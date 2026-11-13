//! The one door a WebAssembly plugin has to the network: `cluaiz.http_fetch`.
//!
//! A plugin gets it by declaring the `network` permission in its `package.json`: `full` access
//! reaches any public host, `strict` only its `hosts`. Only public addresses over https are
//! reachable, and every redirect is checked again. The plugin never sees the user's keys: it writes
//! `{{secret:NAME}}` in a header and the Engine fills it in, for a name in the manifest's `env`.
//! `{{url:NAME}}` at the start of a URL means an address the user chose in such a setting, such as
//! their own SearXNG, which alone may be local.
//!
//! Saved values are per tool, in `config/tool_settings.json`: `{ "<tool id>": { "NAME": "value" } }`.
//! A value in the system environment comes first (see `crate::tool_env`).

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, LOCATION};
use reqwest::{redirect::Policy, Method, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};


const MAX_BODY_BYTES: u64 = 5 * 1024 * 1024;
const MAX_REQUESTS: u32 = 16;
const MAX_REDIRECTS: u32 = 5;
const CALL_DEADLINE: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a request did not happen. `code` is for the plugin to act on
struct Refused {
    code: &'static str,
    message: String,
}

fn refuse<T>(code: &'static str, message: impl Into<String>) -> Result<T, Refused> {
    Err(Refused { code, message: message.into() })
}

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    method: Option<String>,
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    max_bytes: Option<u64>,
}

/// What one plugin call may reach, and how much of it it has used
pub struct NetAccess {
    allowed: bool,
    /// Set when the plugin declares the network but the user refused it
    off: Option<String>,
    /// When set, the only hosts reachable (`api.example.com`, `*.example.com`)
    hosts: Option<Vec<String>>,
    /// The names in the manifest's `env`: the only ones `{{secret:NAME}}` and `{{url:NAME}}` may use
    names: Vec<String>,
    /// Their values: the system environment first, then the user's saved values
    settings: HashMap<String, String>,
    requests: u32,
    started: Instant,
}

impl NetAccess {
    pub fn none() -> Self {
        Self::new(Vec::new(), HashMap::new())
    }

    /// `plugin_id` is the installed folder's name, so a plugin reads only its own settings.
    /// `manifest` is the plugin's `package.json`; the user's permission choices apply to it.
    pub fn for_plugin(plugin_id: &str, manifest: &Value, config_dir: &Path) -> Self {
        use crate::permissions as perm;
        let declared = perm::effective(config_dir, "plugin", plugin_id, manifest);
        let network = declared.iter().find(|p| p.kind == "network");
        let allowed = network.is_some_and(|p| p.on);
        let names = crate::tool_env::declared(manifest).into_iter().map(|setting| setting.name).collect();
        let settings = if allowed { crate::tool_env::values(plugin_id, manifest, config_dir) } else { HashMap::new() };
        let mut access = Self::new(names, settings);
        access.allowed = allowed;
        access.off = network.filter(|p| !p.on).map(|_| perm::off_message("plugin", plugin_id, "network"));
        access.hosts = network.and_then(|p| p.strict_hosts());
        access
    }

    /// For a flow's `code` step: only these hosts (`permissions.network`), no secrets or settings
    pub fn for_hosts(hosts: Vec<String>) -> Self {
        let mut access = Self::none();
        // Every request is checked against `hosts`; an empty list reaches nothing
        access.allowed = true;
        access.hosts = Some(hosts);
        access
    }

    fn host_allowed(&self, url: &Url) -> bool {
        let Some(hosts) = &self.hosts else { return true };
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        hosts.iter().any(|pattern| {
            let pattern = pattern.to_ascii_lowercase();
            match pattern.strip_prefix("*.") {
                Some(domain) => host.ends_with(&format!(".{domain}")),
                None => host == pattern,
            }
        })
    }

    fn new(names: Vec<String>, settings: HashMap<String, String>) -> Self {
        Self {
            allowed: false,
            off: None,
            hosts: None,
            names,
            settings,
            requests: 0,
            started: Instant::now(),
        }
    }

    /// Always answers with JSON: `{status, final_url, content_type, body, truncated}` or `{code, error}`
    pub fn fetch(&mut self, raw: &[u8]) -> Value {
        self.try_fetch(raw).unwrap_or_else(|refused| json!({ "code": refused.code, "error": refused.message }))
    }

    fn try_fetch(&mut self, raw: &[u8]) -> Result<Value, Refused> {
        if let Some(message) = &self.off {
            return refuse("permission_off", message.clone());
        }
        if !self.allowed {
            return refuse("no_permission", "This plugin does not have network access in its package.json");
        }
        let request: Request = serde_json::from_slice(raw)
            .or_else(|e| refuse("bad_request", format!("The request is not valid: {e}")))?;
        if self.requests >= MAX_REQUESTS {
            return refuse("limit", format!("A plugin call may make {MAX_REQUESTS} requests at most"));
        }
        let remaining = CALL_DEADLINE.saturating_sub(self.started.elapsed());
        if remaining.is_zero() {
            return refuse("limit", "This plugin call ran out of time");
        }
        self.requests += 1;

        let (mut url, trusted) = self.resolve_url(&request.url)?;
        let mut headers = self.resolve_headers(&request.headers, &url)?;
        let mut method = match request.method.as_deref().unwrap_or("GET") {
            "GET" => Method::GET,
            "POST" => Method::POST,
            other => return refuse("bad_request", format!("Method {other} is not supported")),
        };
        let mut body = request.body;
        let limit = request.max_bytes.unwrap_or(MAX_BODY_BYTES).min(MAX_BODY_BYTES);
        let timeout = remaining.min(REQUEST_TIMEOUT);

        for _ in 0..=MAX_REDIRECTS {
            if !self.host_allowed(&url) {
                return refuse("no_permission", format!("{} is not in the flow's permissions.network", url.host_str().unwrap_or_default()));
            }
            let addresses = check_target(&url, trusted.as_ref())?;
            let client = client(&url, &addresses, timeout)?;
            let mut outgoing = client.request(method.clone(), url.clone()).headers(headers.clone());
            if let Some(text) = &body {
                outgoing = outgoing.body(text.clone());
            }
            let response = outgoing.send().or_else(|e| refuse("network", e.without_url().to_string()))?;

            let status = response.status();
            let location = response.headers().get(LOCATION).and_then(|v| v.to_str().ok()).map(str::to_owned);
            if let (true, Some(location)) = (status.is_redirection(), location) {
                let next = url.join(&location).or_else(|e| refuse("network", format!("Bad redirect: {e}")))?;
                if next.host_str() != url.host_str() {
                    // A key or anything else the plugin set goes no further than its own host
                    headers = HeaderMap::new();
                }
                if status.as_u16() == 303 || (matches!(status.as_u16(), 301 | 302) && method == Method::POST) {
                    method = Method::GET;
                    body = None;
                }
                url = next;
                continue;
            }

            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let mut bytes = Vec::new();
            response
                .take(limit + 1)
                .read_to_end(&mut bytes)
                .or_else(|e| refuse("network", e.to_string()))?;
            let truncated = bytes.len() as u64 > limit;
            bytes.truncate(limit as usize);
            return Ok(json!({
                "status": status.as_u16(),
                "final_url": url.as_str(),
                "content_type": content_type,
                "body": String::from_utf8_lossy(&bytes),
                "truncated": truncated,
            }));
        }
        refuse("network", format!("More than {MAX_REDIRECTS} redirects"))
    }

    /// The URL to call, and the host and port the user chose, when `{{url:NAME}}` was used
    fn resolve_url(&self, raw: &str) -> Result<(Url, Option<(String, u16)>), Refused> {
        let Some(rest) = raw.strip_prefix("{{url:") else {
            let url = Url::parse(raw).or_else(|e| refuse("bad_request", format!("Bad URL: {e}")))?;
            return Ok((url, None));
        };
        let Some((name, path)) = rest.split_once("}}") else {
            return refuse("bad_request", "An unfinished {{url:NAME}}");
        };
        if !self.names.iter().any(|allowed| allowed == name) {
            return refuse("no_permission", format!("{name} is not in the plugin's env"));
        }
        let Some(base) = self.settings.get(name).filter(|value| !value.trim().is_empty()) else {
            return refuse("missing_setting", format!("{name} is not set"));
        };
        let url = Url::parse(&format!("{}{path}", base.trim().trim_end_matches('/')))
            .or_else(|e| refuse("bad_request", format!("{name} is not a valid URL: {e}")))?;
        let trusted = match (url.host_str(), url.port_or_known_default()) {
            (Some(host), Some(port)) => (host.to_owned(), port),
            _ => return refuse("bad_request", format!("{name} has no host")),
        };
        Ok((url, Some(trusted)))
    }

    fn resolve_headers(&self, wanted: &HashMap<String, String>, url: &Url) -> Result<HeaderMap, Refused> {
        let mut headers = HeaderMap::new();
        for (name, value) in wanted {
            // The Engine says who is calling, and a plugin does not choose where it connects
            if name.eq_ignore_ascii_case("user-agent") || name.eq_ignore_ascii_case("host") {
                continue;
            }
            let filled = self.fill_secrets(value, url)?;
            let name = HeaderName::from_bytes(name.as_bytes())
                .or_else(|_| refuse("bad_request", format!("Bad header name: {name}")))?;
            let mut value = HeaderValue::from_str(&filled)
                .or_else(|_| refuse("bad_request", format!("Bad value for header {name}")))?;
            value.set_sensitive(true);
            headers.insert(name, value);
        }
        Ok(headers)
    }

    /// A key goes to the request's host only if that host may be reached: any public host with
    /// `full` access, a listed one with `strict`. A redirect to another host drops the headers.
    fn fill_secrets(&self, text: &str, url: &Url) -> Result<String, Refused> {
        let mut filled = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("{{secret:") {
            filled.push_str(&rest[..start]);
            let after = &rest[start + "{{secret:".len()..];
            let Some((name, tail)) = after.split_once("}}") else {
                return refuse("bad_request", "An unfinished {{secret:NAME}}");
            };
            if !self.names.iter().any(|allowed| allowed == name) {
                return refuse("no_permission", format!("{name} is not in the plugin's env"));
            }
            if !self.host_allowed(url) {
                return refuse("no_permission", format!("{name} may not be sent to {}", url.host_str().unwrap_or_default()));
            }
            let Some(value) = self.settings.get(name).filter(|value| !value.trim().is_empty()) else {
                return refuse("missing_setting", format!("{name} is not set"));
            };
            filled.push_str(value.trim());
            rest = tail;
        }
        filled.push_str(rest);
        Ok(filled)
    }
}

/// Sets one of a plugin's values, or removes it when `value` is empty. Only the user's own command
/// calls this: a plugin has no way to write here.
pub fn save_setting(plugin_id: &str, name: &str, value: &str, config_dir: &Path) -> std::io::Result<()> {
    let path = config_dir.join("tool_settings.json");
    let mut all: serde_json::Map<String, Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let section = all.entry(plugin_id).or_insert_with(|| json!({}));
    if !section.is_object() {
        *section = json!({});
    }
    if let Some(section) = section.as_object_mut() {
        if value.is_empty() {
            section.remove(name);
        } else {
            section.insert(name.to_owned(), json!(value));
        }
    }
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(&path, serde_json::to_vec_pretty(&all)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// A value a tool asks the user for, named in its manifest's `env`. The value itself is never part
/// of it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct UserSetting {
    pub name: String,
    /// One sentence from the package: what the value is
    pub info: String,
    /// The tool cannot work without it
    pub required: bool,
    /// Typed hidden and never shown. Every value is treated as a key
    pub secret: bool,
    /// The user has saved it, or the system environment has it
    pub set: bool,
    /// The system environment has it, which comes before a saved value
    pub in_system: bool,
}

/// The values the tool in `dir` asks the user for, sorted by name: the names in its `env`. The
/// same rule holds for every tool type.
pub fn user_settings(_kind: &str, dir: &Path, config_dir: &Path) -> Vec<UserSetting> {
    let manifest = crate::tool_env::manifest_in(dir);
    let id = dir.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let saved = settings_set(&id, config_dir);
    crate::tool_env::declared(&manifest)
        .into_iter()
        .map(|setting| {
            let in_system = crate::tool_env::in_system(&setting.name);
            UserSetting {
                set: in_system || saved.contains(&setting.name),
                in_system,
                secret: true,
                required: setting.required,
                info: setting.info,
                name: setting.name,
            }
        })
        .collect()
}

/// Saves one value the tool in `dir` asks for (empty removes it). Err names the values it asks
/// for when `name` is not one of them.
pub fn save_user_setting(kind: &str, dir: &Path, name: &str, value: &str, config_dir: &Path) -> Result<(), String> {
    let declared = user_settings(kind, dir, config_dir);
    if !declared.iter().any(|setting| setting.name == name) {
        let names: Vec<&str> = declared.iter().map(|setting| setting.name.as_str()).collect();
        return Err(if names.is_empty() {
            "this tool asks for no settings".to_string()
        } else {
            format!("'{name}' is not one of this tool's settings: {}", names.join(", "))
        });
    }
    let id = dir.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    save_setting(&id, name, value.trim(), config_dir).map_err(|error| error.to_string())
}

/// The names of a plugin's values that are set. The values stay out of it
pub fn settings_set(plugin_id: &str, config_dir: &Path) -> Vec<String> {
    read_settings(plugin_id, config_dir).into_keys().collect()
}

pub(crate) fn read_settings(plugin_id: &str, config_dir: &Path) -> HashMap<String, String> {
    crate::tool_env::saved(plugin_id, config_dir)
}

/// The addresses to connect to, already checked. Connecting to these, not to a second lookup,
/// means a name cannot answer with a public address here and a local one afterwards.
fn check_target(url: &Url, trusted: Option<&(String, u16)>) -> Result<Vec<SocketAddr>, Refused> {
    let (Some(host), Some(port)) = (url.host_str(), url.port_or_known_default()) else {
        return refuse("blocked", "The URL has no host");
    };
    if !url.username().is_empty() || url.password().is_some() {
        return refuse("blocked", "A URL with a user name or password is not allowed");
    }
    let is_trusted = trusted.is_some_and(|(trusted_host, trusted_port)| trusted_host == host && *trusted_port == port);
    match url.scheme() {
        "https" => {}
        "http" if is_trusted => {}
        scheme => return refuse("blocked", format!("Only https is allowed, not {scheme}")),
    }
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    let addresses: Vec<SocketAddr> = (bare_host, port)
        .to_socket_addrs()
        .or_else(|e| refuse("network", format!("Could not find {host}: {e}")))?
        .collect();
    if addresses.is_empty() {
        return refuse("network", format!("Could not find {host}"));
    }
    if !is_trusted && addresses.iter().any(|address| !is_public(address.ip())) {
        return refuse("blocked", format!("{host} is a local or private address"));
    }
    Ok(addresses)
}

fn client(url: &Url, addresses: &[SocketAddr], timeout: Duration) -> Result<Client, Refused> {
    let host = url.host_str().unwrap_or_default();
    Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .user_agent(format!("Cluaiz/{} (+https://cluaiz.com)", cluaiz_core::VERSION))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(timeout)
        .resolve_to_addrs(host, addresses)
        .build()
        .or_else(|e| refuse("network", e.to_string()))
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..128).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..32).contains(&b))
                || (a == 192 && b == 0 && c == 0)
                || (a == 192 && b == 0 && c == 2)
                || (a == 192 && b == 168)
                || (a == 198 && (b == 18 || b == 19))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let segments = v6.segments();
            let first = segments[0];
            // NAT64: many mobile networks answer every name this way, with the IPv4 address inside
            if first == 0x0064 && segments[1] == 0xff9b && segments[2..6] == [0, 0, 0, 0] {
                let [.., a, b, c, d] = v6.octets();
                return is_public(IpAddr::V4(std::net::Ipv4Addr::new(a, b, c, d)));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
                || first & 0xff00 == 0xff00
                || first == 0x0064
                || first == 0x2002
                || (first == 0x2001 && v6.segments()[1] == 0x0db8))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    #[test]
    fn the_users_choice_switches_the_network() {
        let dir = std::env::temp_dir().join(format!("cluaiz-net-perm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = json!({"permissions": {"network": {"info": "x", "access": "strict", "hosts": ["api.example.com"]}}});
        let on = NetAccess::for_plugin("p", &manifest, &dir);
        assert!(on.allowed);
        assert!(!on.host_allowed(&Url::parse("https://other.example.org/").unwrap()), "strict access limits the plugin");
        let full = json!({"permissions": {"network": {"info": "x", "access": "full", "hosts": ["api.example.com"]}}});
        assert!(NetAccess::for_plugin("p", &full, &dir).host_allowed(&Url::parse("https://other.example.org/").unwrap()), "full access reaches any host");
        crate::permissions::set(&dir, "plugin", "p", "network", false).unwrap();
        let mut off = NetAccess::for_plugin("p", &manifest, &dir);
        let answer = off.fetch(br#"{"url":"https://api.example.com/"}"#);
        assert_eq!(answer["code"], "permission_off");
        assert!(answer["error"].as_str().unwrap().contains("cluaiz plugin permission p network on"));
        let undeclared = NetAccess::for_plugin("q", &json!({}), &dir).fetch(br#"{"url":"https://api.example.com/"}"#);
        assert_eq!(undeclared["code"], "no_permission");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_flow_module_reaches_only_its_hosts() {
        let mut access = NetAccess::for_hosts(vec!["api.example.com".into(), "*.github.com".into()]);
        let refused = access.fetch(br#"{"url":"https://evil.example.net/x"}"#);
        assert_eq!(refused["code"], "no_permission");
        assert!(access.host_allowed(&Url::parse("https://api.github.com/").unwrap()));
        assert!(!access.host_allowed(&Url::parse("https://github.com.evil.net/").unwrap()));
        assert_eq!(NetAccess::for_hosts(vec![]).fetch(br#"{"url":"https://api.example.com/"}"#)["code"], "no_permission");
    }

    fn access(settings: &[(&str, &str)]) -> NetAccess {
        let settings = settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let mut access = NetAccess::new(vec!["KEY".to_string(), "OWN_URL".to_string()], settings);
        access.allowed = true;
        access
    }

    #[test]
    fn a_key_goes_only_where_the_plugin_may_reach() {
        let url = Url::parse("https://api.example.com/").unwrap();
        let mut strict = access(&[("KEY", "abc")]);
        strict.hosts = Some(vec!["api.example.com".into()]);
        assert_eq!(strict.fill_secrets("Bearer {{secret:KEY}}", &url).ok().as_deref(), Some("Bearer abc"));
        let other = Url::parse("https://evil.example.net/").unwrap();
        assert!(strict.fill_secrets("{{secret:KEY}}", &other).is_err(), "strict access keeps a key to its hosts");
        assert!(strict.fill_secrets("{{secret:UNLISTED}}", &url).is_err(), "a name not in env is refused");
    }

    fn code(answer: &Value) -> &str {
        answer["code"].as_str().unwrap_or("")
    }

    #[test]
    fn a_plugin_without_the_permission_cannot_call() {
        let answer = NetAccess::none().fetch(br#"{"url":"https://example.com"}"#);
        assert_eq!(code(&answer), "no_permission");
    }

    #[test]
    fn local_and_private_addresses_are_refused() {
        for url in [
            "https://127.0.0.1/",
            "https://localhost/",
            "https://0x7f.1/",
            "https://10.0.0.1/",
            "https://169.254.169.254/latest/meta-data/",
            "https://192.168.1.1/",
            "https://[::1]/",
            "https://[::ffff:127.0.0.1]/",
        ] {
            let answer = access(&[]).fetch(json!({ "url": url }).to_string().as_bytes());
            assert_eq!(code(&answer), "blocked", "{url}: {answer}");
        }
    }

    #[test]
    fn a_nat64_address_counts_as_the_address_inside_it() {
        assert!(is_public("64:ff9b::2cd7:2032".parse().unwrap()));
        assert!(!is_public("64:ff9b::7f00:1".parse().unwrap()));
        assert!(!is_public("64:ff9b::a9fe:a9fe".parse().unwrap()));
        assert!(!is_public("64:ff9b:1::1".parse().unwrap()));
    }

    #[test]
    fn plain_http_and_other_schemes_are_refused() {
        for url in ["http://example.com/", "file:///etc/passwd", "ftp://example.com/"] {
            let answer = access(&[]).fetch(json!({ "url": url }).to_string().as_bytes());
            assert_eq!(code(&answer), "blocked", "{url}: {answer}");
        }
    }

    #[test]
    fn a_url_with_a_password_is_refused() {
        let answer = access(&[]).fetch(br#"{"url":"https://me:secret@example.com/"}"#);
        assert_eq!(code(&answer), "blocked");
    }

    #[test]
    fn a_secret_that_is_not_set_is_reported_without_a_request() {
        let answer = access(&[]).fetch(
            br#"{"url":"https://api.example.com/","headers":{"Authorization":"Bearer {{secret:KEY}}"}}"#,
        );
        assert_eq!(code(&answer), "missing_setting");
    }

    #[test]
    fn requests_per_call_are_limited() {
        let mut net = access(&[]);
        for _ in 0..MAX_REQUESTS {
            net.fetch(br#"{"url":"https://127.0.0.1/"}"#);
        }
        assert_eq!(code(&net.fetch(br#"{"url":"https://127.0.0.1/"}"#)), "limit");
    }

    /// Answers each connection in turn with the given raw responses
    fn local_server(responses: Vec<String>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 4096];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        port
    }

    #[test]
    fn the_address_the_user_chose_may_be_local() {
        let port = local_server(vec!["HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".into()]);
        let base = format!("http://127.0.0.1:{port}/");
        let answer = access(&[("OWN_URL", base.as_str())]).fetch(br#"{"url":"{{url:OWN_URL}}/search?q=x"}"#);
        assert_eq!(answer["status"], 200, "{answer}");
        assert_eq!(answer["body"], "ok");
    }

    #[test]
    fn a_redirect_cannot_leave_the_chosen_address_for_another_local_one() {
        let redirect = "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let port = local_server(vec![redirect.into()]);
        let base = format!("http://127.0.0.1:{port}");
        let answer = access(&[("OWN_URL", base.as_str())]).fetch(br#"{"url":"{{url:OWN_URL}}/"}"#);
        assert_eq!(code(&answer), "blocked", "{answer}");
    }

    #[test]
    fn a_tool_is_asked_only_for_the_settings_its_manifest_names() {
        let root = std::env::temp_dir().join(format!("cluaiz-user-settings-{}", uuid::Uuid::new_v4()));
        let (plugin, server, config) = (root.join("search"), root.join("docs"), root.join("config"));
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::create_dir_all(&server).unwrap();
        std::fs::write(
            plugin.join("package.json"),
            r#"{"id":"search","permissions":{"network":{"info":"x","access":"full"}},"env":{"KEY":{"info":"a key","need":"required"},"BASE_URL":{"info":"your server"}}}"#,
        )
        .unwrap();
        std::fs::write(server.join("package.json"), r#"{"id":"docs","env":{"TOKEN":{"info":"a token"}}}"#).unwrap();

        let names = |kind: &str, dir: &Path| -> Vec<(String, bool, bool)> {
            user_settings(kind, dir, &config).into_iter().map(|s| (s.name, s.required, s.set)).collect()
        };
        assert_eq!(names("plugin", &plugin), [("BASE_URL".into(), false, false), ("KEY".into(), true, false)]);
        assert_eq!(names("mcp", &server), [("TOKEN".into(), false, false)]);
        assert_eq!(names("skill", &plugin).len(), 2, "every tool type follows the same rule");

        save_user_setting("plugin", &plugin, "KEY", " abc ", &config).unwrap();
        assert_eq!(read_settings("search", &config).get("KEY").map(String::as_str), Some("abc"));
        assert!(names("plugin", &plugin).contains(&("KEY".into(), true, true)));
        // A name the manifest does not give is refused, and nothing is written
        let refused = save_user_setting("plugin", &plugin, "OTHER", "x", &config).unwrap_err();
        assert!(refused.contains("BASE_URL, KEY"), "{refused}");
        assert!(read_settings("search", &config).get("OTHER").is_none());
        // Empty removes it
        save_user_setting("plugin", &plugin, "KEY", "", &config).unwrap();
        assert!(read_settings("search", &config).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_saved_setting_reaches_only_its_own_plugin() {
        let folder = std::env::temp_dir().join(format!("cluaiz-net-{}", uuid::Uuid::new_v4()));
        save_setting("one", "KEY", "abc", &folder).unwrap();
        save_setting("two", "OTHER", "def", &folder).unwrap();
        assert_eq!(read_settings("one", &folder).get("KEY").map(String::as_str), Some("abc"));
        assert!(read_settings("one", &folder).get("OTHER").is_none());
        save_setting("one", "KEY", "", &folder).unwrap();
        assert!(settings_set("one", &folder).is_empty());
        assert_eq!(settings_set("two", &folder), ["OTHER"]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_large_answer_is_cut_at_the_limit() {
        let port = local_server(vec!["HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789".into()]);
        let base = format!("http://127.0.0.1:{port}");
        let answer =
            access(&[("OWN_URL", base.as_str())]).fetch(br#"{"url":"{{url:OWN_URL}}/","max_bytes":4}"#);
        assert_eq!(answer["body"], "0123");
        assert_eq!(answer["truncated"], true);
    }
}
