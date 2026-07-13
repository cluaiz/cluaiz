//! archer-server: The cluaiz Telemetry Bridge.
//! Bare-metal HTTP implementation over Tokio for 0.0ms engine impact.

use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use cluaiz_shared::hardware::telemetry::ObservableHardwareState;
use std::sync::atomic::Ordering;

pub struct TelemetryServer {
    state: Arc<ObservableHardwareState>,
}

impl TelemetryServer {
    pub fn new(state: Arc<ObservableHardwareState>) -> Self {
        Self { state }
    }

    pub async fn start(self, port: u16) -> anyhow::Result<()> {
        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr).await?;

        loop {
            let (stream, _) = listener.accept().await?;
            let state = self.state.clone();
            tokio::spawn(async move {
                let _ = handle_connection(stream, state).await;
            });
        }
    }
}

async fn handle_connection(mut stream: TcpStream, state: Arc<ObservableHardwareState>) -> anyhow::Result<()> {
    let mut request = String::new();
    let mut buffer = [0; 8192];
    loop {
        let n = stream.read(&mut buffer).await?;
        if n == 0 { break; }
        request.push_str(&String::from_utf8_lossy(&buffer[..n]));
        
        if let Some(idx) = request.find("\r\n\r\n") {
            let headers = &request[..idx];
            let body_start = idx + 4;
            let current_body_len = request.len() - body_start;
            
            if let Some(cl_idx) = headers.to_lowercase().find("content-length:") {
                let cl_str = &headers[cl_idx + 15..];
                let cl_end = cl_str.find("\r\n").unwrap_or(cl_str.len());
                if let Ok(cl) = cl_str[..cl_end].trim().parse::<usize>() {
                    if current_body_len >= cl {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    
    println!("--- RECEIVED HTTP REQUEST ---\n{:?}\n-----------------------------", request);

    if request.starts_with("GET /api/stats") {
        let json_payload = {
            let pulse = state.pulse.read().unwrap();
            format!(
                "{{\"vram\": {}, \"relay\": {:.2}, \"cache\": {}, \"disk\": {}, \"cores\": {:?}}}",
                pulse.vram_pressure_pct,
                pulse.relay_latency_ms as f64 / 10.0,
                pulse.kv_cache_footprint_mb,
                pulse.storage_throughput_mbps,
                pulse.per_core_usage
            )
        };

        let response_header = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n";
        let response = format!(
            "{}Content-Length: {}\r\n\r\n{}",
            response_header, json_payload.len(), json_payload
        );
        stream.write_all(response.as_bytes()).await?;
    } 
    else if request.starts_with("GET /dashboard") {
        let dashboard_path = cluaiz_shared::environment::EnvironmentManager::current().local_dir.join("assets/cluaiz_Dashboard.html");
        let dashboard_html = std::fs::read_to_string(dashboard_path).unwrap_or_else(|_| "<h1>Dashboard not found</h1>".to_string());
        let response_header = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n";
        let response = format!(
            "{}Content-Length: {}\r\n\r\n{}",
            response_header, dashboard_html.len(), dashboard_html
        );
        stream.write_all(response.as_bytes()).await?;
    }
    else if request.starts_with("POST /api/control/turbo") {
        let is_turbo = request.contains("state=true");
        state.turbo_quant_enabled.store(is_turbo, Ordering::Release);
        
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        stream.write_all(response.as_bytes()).await?;
    }
    else if request.starts_with("GET /api/components/list") {
        match handle_components_list().await {
            Ok(json) => {
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}", json.len(), json);
                stream.write_all(response.as_bytes()).await?;
            },
            Err(e) => {
                let err_resp = format!("HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\r\n{{\"error\":\"{}\"}}", e);
                stream.write_all(err_resp.as_bytes()).await?;
            }
        }
    }
    else if request.starts_with("GET /api/components/settings") {
        let mut comp_type = None;
        let mut comp_id = None;
        if let Some(path_end) = request[4..].find(' ') {
            let path = &request[4..4+path_end];
            if let Some(query_start) = path.find('?') {
                let query = &path[query_start+1..];
                for pair in query.split('&') {
                    let mut kv = pair.split('=');
                    if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                        if k == "type" { comp_type = Some(v.to_string()); }
                        if k == "id" { comp_id = Some(v.to_string()); }
                    }
                }
            }
        }
        
        if let (Some(t), Some(id)) = (comp_type, comp_id) {
            match handle_settings_read(&t, &id).await {
                Ok(json) => {
                    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}", json.len(), json);
                    stream.write_all(response.as_bytes()).await?;
                },
                Err(e) => {
                    let err_resp = format!("HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\r\n{{\"error\":\"{}\"}}", e);
                    stream.write_all(err_resp.as_bytes()).await?;
                }
            }
        } else {
            let err_resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\r\n{\"error\":\"missing type or id\"}";
            stream.write_all(err_resp.as_bytes()).await?;
        }
    }
    else if request.starts_with("POST /api/components/settings") {
        if let Some(body_start) = request.find("\r\n\r\n") {
            let body = &request[body_start + 4..];
            if let Some(json_start) = body.find('{') {
                if let Some(json_end) = body.rfind('}') {
                    let json_str = &body[json_start..=json_end];
                    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(json_str) {
                        match handle_settings_update(payload).await {
                            Ok(_) => {
                                let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\r\n{\"status\":\"success\"}";
                                stream.write_all(response.as_bytes()).await?;
                            },
                            Err(e) => {
                                let err_resp = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\r\n{{\"status\":\"error\", \"message\":\"{}\"}}", e);
                                stream.write_all(err_resp.as_bytes()).await?;
                            }
                        }
                        return Ok(());
                    }
                }
            }
        }
        let response = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
        stream.write_all(response.as_bytes()).await?;
    }
    else if request.starts_with("OPTIONS") {
        let response = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nContent-Length: 0\r\n\r\n";
        stream.write_all(response.as_bytes()).await?;
    }
    else {
        let response = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 0\r\n\r\n";
        stream.write_all(response.as_bytes()).await?;
    }

    Ok(())
}

fn json_to_yaml(json: &serde_json::Value) -> serde_yaml::Value {
    match json {
        serde_json::Value::Null => serde_yaml::Value::Null,
        serde_json::Value::Bool(b) => serde_yaml::Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_yaml::Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                serde_yaml::Value::Number(f.into())
            } else {
                serde_yaml::Value::Null
            }
        },
        serde_json::Value::String(s) => serde_yaml::Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            let mut y_arr = Vec::new();
            for item in arr {
                y_arr.push(json_to_yaml(item));
            }
            serde_yaml::Value::Sequence(y_arr)
        },
        serde_json::Value::Object(obj) => {
            let mut y_map = serde_yaml::Mapping::new();
            for (k, v) in obj {
                y_map.insert(serde_yaml::Value::String(k.clone()), json_to_yaml(v));
            }
            serde_yaml::Value::Mapping(y_map)
        }
    }
}

async fn handle_components_list() -> Result<String, String> {
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let mut results = serde_json::Map::new();
    
    for comp_type in ["extension", "plugin", "mcp", "skill"] {
        let dir = env.global_dir.join(format!("{}s", comp_type));
        let mut items = Vec::new();
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        items.push(serde_json::Value::String(entry.file_name().to_string_lossy().to_string()));
                    }
                }
            }
        }
        results.insert(comp_type.to_string(), serde_json::Value::Array(items));
    }
    
    serde_json::to_string(&serde_json::Value::Object(results)).map_err(|e| e.to_string())
}

fn yaml_to_json(y: serde_yaml::Value) -> serde_json::Value {
    match y {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        },
        serde_yaml::Value::String(s) => serde_json::Value::String(s),
        serde_yaml::Value::Sequence(arr) => {
            serde_json::Value::Array(arr.into_iter().map(yaml_to_json).collect())
        },
        serde_yaml::Value::Mapping(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                let k_str = match k {
                    serde_yaml::Value::String(s) => s,
                    other => serde_yaml::to_string(&other).unwrap_or_default().trim().to_string(),
                };
                obj.insert(k_str, yaml_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        serde_yaml::Value::Tagged(t) => yaml_to_json(t.value),
    }
}

async fn handle_settings_read(component_type: &str, component_id: &str) -> Result<String, String> {
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let comp_dir = env.global_dir.join(format!("{}s", component_type)).join(component_id);
    
    let possible_manifests = [
        format!("manifest-{}.yaml", component_type),
        format!("manifest-{}.yml", component_type),
        "SKILL.md".to_string()
    ];

    let mut target_manifest = None;
    for f in possible_manifests {
        let p = comp_dir.join(&f);
        if p.exists() {
            target_manifest = Some(p);
            break;
        }
    }

    let target_manifest = target_manifest.ok_or("No manifest file found")?;
    let content = std::fs::read_to_string(&target_manifest).map_err(|e| e.to_string())?;
    
    let is_skill_md = target_manifest.to_string_lossy().ends_with("SKILL.md");
    let yaml_str = if is_skill_md {
        let mut parts = content.splitn(3, "---");
        parts.next();
        parts.next().ok_or("Invalid SKILL.md frontmatter")?.to_string()
    } else {
        content.clone()
    };

    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&yaml_str).map_err(|e| format!("YAML Parse error: {}", e))?;
    let json_value = yaml_to_json(yaml_value);
    serde_json::to_string(&json_value).map_err(|e| e.to_string())
}

async fn handle_settings_update(payload: serde_json::Value) -> Result<(), String> {
    let component_type = payload.get("component_type").and_then(|v| v.as_str()).ok_or("Missing component_type")?;
    let component_id = payload.get("component_id").and_then(|v| v.as_str()).ok_or("Missing component_id")?;
    let updates = payload.get("updates").and_then(|v| v.as_object()).ok_or("Missing updates object")?;

    let env_manager = cluaiz_shared::environment::EnvironmentManager::current();
    let comp_dir = env_manager.global_dir.join(format!("{}s", component_type)).join(component_id);
    
    if !comp_dir.exists() {
        return Err(format!("Component directory does not exist: {:?}", comp_dir));
    }

    let possible_manifests = [
        format!("manifest-{}.yaml", component_type),
        format!("manifest-{}.yml", component_type),
        "SKILL.md".to_string()
    ];

    let mut target_manifest = None;
    for f in possible_manifests {
        let p = comp_dir.join(&f);
        if p.exists() {
            target_manifest = Some(p);
            break;
        }
    }

    let target_manifest = target_manifest.ok_or("No manifest file found")?;
    
    let content = std::fs::read_to_string(&target_manifest).map_err(|e| e.to_string())?;
    
    let is_skill_md = target_manifest.to_string_lossy().ends_with("SKILL.md");
    
    let yaml_str = if is_skill_md {
        let mut parts = content.splitn(3, "---");
        parts.next();
        parts.next().ok_or("Invalid SKILL.md frontmatter")?.to_string()
    } else {
        content.clone()
    };

    let mut yaml_value: serde_yaml::Value = serde_yaml::from_str(&yaml_str).map_err(|e| format!("YAML Parse error: {}", e))?;
    
    if let serde_yaml::Value::Mapping(ref mut root_map) = yaml_value {
        for (section_name, section_updates) in updates {
            let section_key = serde_yaml::Value::String(section_name.to_string());
            
            let section_map = match root_map.get_mut(&section_key) {
                Some(serde_yaml::Value::Mapping(m)) => m,
                Some(_) => return Err(format!("Section {} is not a mapping in YAML", section_name)),
                None => {
                    root_map.insert(section_key.clone(), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                    match root_map.get_mut(&section_key).unwrap() {
                        serde_yaml::Value::Mapping(m) => m,
                        _ => unreachable!(),
                    }
                }
            };

            if let Some(update_map) = section_updates.as_object() {
                for (k, v) in update_map {
                    section_map.insert(serde_yaml::Value::String(k.clone()), json_to_yaml(v));
                }
            }
        }
    } else {
        return Err("YAML root is not a mapping".to_string());
    }

    let new_yaml_str = serde_yaml::to_string(&yaml_value).map_err(|e| e.to_string())?;

    if is_skill_md {
        let mut parts = content.splitn(3, "---");
        parts.next();
        parts.next(); // skip old frontmatter
        let body = parts.next().unwrap_or("");
        let new_content = format!("---\n{}---\n{}", new_yaml_str.trim_start_matches("---\n"), body);
        std::fs::write(&target_manifest, new_content).map_err(|e| e.to_string())?;
    } else {
        std::fs::write(&target_manifest, new_yaml_str).map_err(|e| e.to_string())?;
    }

    Ok(())
}
