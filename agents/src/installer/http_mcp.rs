//! `cluaiz mcp add <name> --url <url> [--header "Name: value"]`: an MCP server that is a URL, not a
//! program. It writes the same `package.json` an installed stdio server has, with an `execution`
//! block of `url` and `headers` (see `mcp/manifest.rs`), into the MCP folder, so listing, removing,
//! `config` and starting work as for any installed MCP server.

use super::tools_client::ToolsInstaller;
use crate::tools::registry::ToolsRegistry;
use anyhow::{anyhow, Result};
use cluaiz_core::EnvironmentManager;
use serde_json::{json, Map, Value};
use std::path::Path;

/// What `add_http_mcp` wrote
#[derive(Debug, Clone)]
pub struct AddedHttpMcp {
    pub id: String,
    pub url: String,
    /// Settings the headers name with `${NAME}`: the user sets each with `cluaiz mcp config <id> <NAME>`
    pub settings: Vec<String>,
}

/// `"Name: value"` as a header pair, split at the first colon.
pub fn parse_header(text: &str) -> Result<(String, String)> {
    let (name, value) = text
        .split_once(':')
        .ok_or_else(|| anyhow!("Header '{}' must be written Name: value.", text.trim()))?;
    let (name, value) = (name.trim(), value.trim());
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)) {
        return Err(anyhow!("'{}' is not a valid header name.", name));
    }
    // The value is never repeated in an error: it can be a secret
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(anyhow!("Header '{}' needs a value without control characters.", name));
    }
    Ok((name.to_string(), value.to_string()))
}

/// The `${NAME}` names in a header value, in order, once each
fn setting_names(value: &str) -> Result<Vec<String>> {
    let mut names = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        let Some(len) = rest[start + 2..].find('}') else {
            return Err(anyhow!("A '${{' in a header value has no closing '}}'."));
        };
        let name = &rest[start + 2..start + 2 + len];
        let mut chars = name.chars();
        let valid = chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if !valid {
            return Err(anyhow!("'${{{}}}' is not a setting name (upper case letters, digits and _, starting with a letter).", name));
        }
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
        rest = &rest[start + 3 + len..];
    }
    Ok(names)
}

/// The package.json of an HTTP MCP server, and the settings its headers name
pub fn http_package(id: &str, url: &str, headers: &[(String, String)]) -> Result<(Value, Vec<String>)> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|e| anyhow!("'{}' is not a valid URL: {}", url, e))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(anyhow!("The MCP address must be an http:// or https:// URL."));
    }
    let mut header_map = Map::new();
    let mut settings: Vec<String> = Vec::new();
    for (name, value) in headers {
        if header_map.contains_key(name) {
            return Err(anyhow!("Header '{}' is given twice.", name));
        }
        for setting in setting_names(value)? {
            if !settings.contains(&setting) {
                settings.push(setting);
            }
        }
        header_map.insert(name.clone(), Value::String(value.clone()));
    }
    let mut package = json!({
        "id": id,
        "name": id,
        "version": "1.0.0",
        "description": format!("MCP server at {}", parsed.origin().ascii_serialization()),
        "type": "mcp",
        "execution": { "url": parsed.as_str() }
    });
    if !header_map.is_empty() {
        package["execution"]["headers"] = Value::Object(header_map);
    }
    if !settings.is_empty() {
        let env: Map<String, Value> = settings
            .iter()
            .map(|name| (name.clone(), json!({ "info": format!("Used in a header of the MCP server '{}'", id), "need": "required" })))
            .collect();
        package["env"] = Value::Object(env);
    }
    Ok((package, settings))
}

/// Writes the package into `mcp_root/<id>`. An existing folder is never replaced.
pub fn write_http_package(mcp_root: &Path, id: &str, url: &str, headers: &[(String, String)]) -> Result<AddedHttpMcp> {
    if !ToolsInstaller::is_valid_component_id(id) {
        return Err(anyhow!("Invalid mcp name '{}'.", id));
    }
    let (package, settings) = http_package(id, url, headers)?;
    let dir = mcp_root.join(id);
    if dir.exists() {
        return Err(anyhow!("mcp '{}' is already installed. Remove it first with: cluaiz mcp remove {}", id, id));
    }
    std::fs::create_dir_all(mcp_root)?;
    // Written whole in a hidden sibling and moved in, so a failure never leaves a folder without package.json
    let staging = mcp_root.join(format!(".adding-{}-{}", id, std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let result = std::fs::write(staging.join("package.json"), serde_json::to_vec_pretty(&package)?)
        .and_then(|_| std::fs::rename(&staging, &dir));
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(anyhow!("Could not write the MCP server '{}': {}", id, e));
    }
    Ok(AddedHttpMcp {
        id: id.to_string(),
        url: package["execution"]["url"].as_str().unwrap_or_default().to_string(),
        settings,
    })
}

impl ToolsInstaller {
    /// Adds an MCP server that is reached by URL (Streamable HTTP). Nothing is downloaded.
    pub fn add_http_mcp(id: &str, url: &str, headers: &[(String, String)]) -> Result<AddedHttpMcp> {
        let env = EnvironmentManager::current();
        let mcp_root = env.ensure_mcp_dir().unwrap_or_else(|_| env.mcp_dir());
        let added = write_http_package(&mcp_root, id, url, headers)?;
        match ToolsRegistry::load() {
            Ok(mut reg) => {
                if let Err(e) = reg.sync_with_filesystem() {
                    tracing::warn!("⚠️ [ToolsEngine] Registry sync after adding '{}' failed: {}", id, e);
                }
                if let Err(e) = reg.save() {
                    tracing::warn!("⚠️ [ToolsEngine] Registry save after adding '{}' failed: {}", id, e);
                }
            }
            Err(e) => tracing::warn!("⚠️ [ToolsEngine] Registry load after adding '{}' failed: {}", id, e),
        }
        Ok(added)
    }

    /// The address of an installed HTTP MCP server, for listings. Header values stay out of it.
    pub fn http_mcp_url(mcp_dir: &Path) -> Option<String> {
        crate::mcp::manifest::McpManifestParser::parse_file(mcp_dir.join("package.json"))?.execution?.url
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::manifest::McpManifestParser;

    fn temp_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("cluaiz-http-mcp-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn header_is_split_at_the_first_colon() {
        assert_eq!(parse_header("Authorization: Bearer ${TOKEN}").unwrap(), ("Authorization".into(), "Bearer ${TOKEN}".into()));
        assert_eq!(parse_header("X-Url: http://a:1").unwrap().1, "http://a:1");
        assert!(parse_header("NoColon").is_err());
        assert!(parse_header("Bad Name: v").is_err());
        assert!(parse_header("X:").is_err());
    }

    #[test]
    fn error_for_a_bad_header_does_not_repeat_the_value() {
        let error = parse_header("Name:\u{7}secret").unwrap_err().to_string();
        assert!(!error.contains("secret"));
    }

    #[test]
    fn written_package_is_read_by_the_mcp_manifest_parser() {
        let root = temp_root();
        let headers = vec![("Authorization".to_string(), "Bearer ${EXAMPLE_TOKEN}".to_string())];
        let added = write_http_package(&root, "remote", "https://example.com/mcp", &headers).unwrap();
        assert_eq!(added.settings, vec!["EXAMPLE_TOKEN".to_string()]);
        let manifest = McpManifestParser::parse_file(root.join("remote/package.json")).unwrap();
        assert_eq!(manifest.name, "remote");
        let exec = manifest.execution.unwrap();
        assert_eq!(exec.url.as_deref(), Some("https://example.com/mcp"));
        assert_eq!(exec.headers.get("Authorization").map(String::as_str), Some("Bearer ${EXAMPLE_TOKEN}"));
        // The header's setting is declared, so `cluaiz mcp config remote` can set it
        let text = std::fs::read_to_string(root.join("remote/package.json")).unwrap();
        let declared = crate::tool_env::declared(&serde_json::from_str(&text).unwrap());
        assert_eq!(declared.len(), 1);
        assert_eq!(declared[0].name, "EXAMPLE_TOKEN");
        assert!(declared[0].required);
        assert_eq!(ToolsInstaller::http_mcp_url(&root.join("remote")).as_deref(), Some("https://example.com/mcp"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_header_means_no_headers_and_no_settings() {
        let root = temp_root();
        let added = write_http_package(&root, "plain", "http://127.0.0.1:9000/mcp", &[]).unwrap();
        assert!(added.settings.is_empty());
        let text = std::fs::read_to_string(root.join("plain/package.json")).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert!(value["execution"].get("headers").is_none());
        assert!(value.get("env").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_bad_urls_names_and_an_existing_server() {
        let root = temp_root();
        assert!(write_http_package(&root, "a", "ftp://example.com/mcp", &[]).is_err());
        assert!(write_http_package(&root, "a", "not a url", &[]).is_err());
        assert!(write_http_package(&root, "../a", "https://example.com/mcp", &[]).is_err());
        assert!(write_http_package(&root, "a", "https://example.com/mcp", &[("X".into(), "${lower}".into())]).is_err());
        write_http_package(&root, "a", "https://example.com/mcp", &[]).unwrap();
        let again = write_http_package(&root, "a", "https://example.com/other", &[]).unwrap_err().to_string();
        assert!(again.contains("already installed"));
        // the first one is untouched and no staging folder is left
        assert_eq!(ToolsInstaller::http_mcp_url(&root.join("a")).as_deref(), Some("https://example.com/mcp"));
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
