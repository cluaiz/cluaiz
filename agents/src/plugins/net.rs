//! The one door a WebAssembly plugin has to the network: `cluaiz.http_fetch`.
//!
//! A plugin gets it by naming `permissions.network_access` in its `package.json`. Only public
//! addresses over https are reachable, and every redirect is checked again. The plugin never sees
//! the user's keys: it writes `{{secret:NAME}}` in a header and the Engine fills it in, only for
//! the host the manifest binds that name to. `{{url:NAME}}` at the start of a URL means an address
//! the user chose in their settings, such as their own SearXNG, which alone may be local.
//!
//! Settings are per plugin, in `config/tool_settings.json`: `{ "<plugin id>": { "NAME": "value" } }`.

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

use super::manifest::PluginPermissions;

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
    secrets: HashMap<String, String>,
    user_urls: Vec<String>,
    settings: HashMap<String, String>,
    requests: u32,
    started: Instant,
}

impl NetAccess {
    pub fn none() -> Self {
        Self::new(None, HashMap::new())
    }

    /// `plugin_id` is the installed folder's name, so a plugin reads only its own settings
    pub fn for_plugin(plugin_id: &str, permissions: Option<&PluginPermissions>, config_dir: &Path) -> Self {
        let allowed = permissions.is_some_and(|p| p.network_access);
        let settings = if allowed { read_settings(plugin_id, config_dir) } else { HashMap::new() };
        Self::new(permissions, settings)
    }

    fn new(permissions: Option<&PluginPermissions>, settings: HashMap<String, String>) -> Self {
        Self {
            allowed: permissions.is_some_and(|p| p.network_access),
            secrets: permissions.map(|p| p.secrets.clone()).unwrap_or_default(),
            user_urls: permissions.map(|p| p.user_urls.clone()).unwrap_or_default(),
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
        if !self.user_urls.iter().any(|allowed| allowed == name) {
            return refuse("no_permission", format!("{name} is not in permissions.user_urls"));
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

    fn fill_secrets(&self, text: &str, url: &Url) -> Result<String, Refused> {
        let mut filled = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("{{secret:") {
            filled.push_str(&rest[..start]);
            let after = &rest[start + "{{secret:".len()..];
            let Some((name, tail)) = after.split_once("}}") else {
                return refuse("bad_request", "An unfinished {{secret:NAME}}");
            };
            match self.secrets.get(name) {
                Some(host) if url.host_str() == Some(host.as_str()) => {}
                Some(host) => return refuse("no_permission", format!("{name} may only be sent to {host}")),
                None => return refuse("no_permission", format!("{name} is not in permissions.secrets")),
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

fn read_settings(plugin_id: &str, config_dir: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(config_dir.join("tool_settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|all| serde_json::from_value(all.get(plugin_id)?.clone()).ok())
        .unwrap_or_default()
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
        .user_agent(concat!("Cluaiz/", env!("CARGO_PKG_VERSION"), " (+https://cluaiz.com)"))
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

    fn access(settings: &[(&str, &str)]) -> NetAccess {
        let permissions = PluginPermissions {
            network_access: true,
            secrets: HashMap::from([("KEY".to_string(), "api.example.com".to_string())]),
            user_urls: vec!["OWN_URL".to_string()],
            ..Default::default()
        };
        let settings = settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        NetAccess::new(Some(&permissions), settings)
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
    fn a_secret_goes_only_to_its_own_host() {
        let net = access(&[("KEY", "abc")]);
        let right = Url::parse("https://api.example.com/x").unwrap();
        let wrong = Url::parse("https://other.example.com/x").unwrap();
        assert_eq!(net.fill_secrets("Bearer {{secret:KEY}}", &right).ok().as_deref(), Some("Bearer abc"));
        assert_eq!(net.fill_secrets("{{secret:KEY}}", &wrong).err().map(|r| r.code), Some("no_permission"));
        assert_eq!(net.fill_secrets("{{secret:OTHER}}", &right).err().map(|r| r.code), Some("no_permission"));
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
    fn a_large_answer_is_cut_at_the_limit() {
        let port = local_server(vec!["HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789".into()]);
        let base = format!("http://127.0.0.1:{port}");
        let answer =
            access(&[("OWN_URL", base.as_str())]).fetch(br#"{"url":"{{url:OWN_URL}}/","max_bytes":4}"#);
        assert_eq!(answer["body"], "0123");
        assert_eq!(answer["truncated"], true);
    }
}
