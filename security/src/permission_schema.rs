//! 🛡️ Master Permission Schema & Dynamic Hardware Capability Registry (`security/src/permission_schema.rs`)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use cluaiz_core::EnvironmentManager;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SlotConfig {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub format_type: Option<String>,
    #[serde(default)]
    pub supported_tasks: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModelSelection {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub vision: Option<String>,
    #[serde(default)]
    pub audio: Option<String>,
}

fn default_connection_protocol() -> String {
    "http".to_string()
}

fn default_api_host() -> String {
    "0.0.0.0".to_string()
}

/// What CEP `system.exec` may do. Both off by default: every call asks the user, and only a
/// program with its arguments runs (no shell line).
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ExecPolicy {
    /// Accept `{"command": "<shell line>"}` (cmd /C, sh -c) besides `{"program", "args"}`
    #[serde(default)]
    pub allow_shell: bool,
    /// Run without the user's approval of each call
    #[serde(default)]
    pub without_approval: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiAuth {
    #[serde(default = "default_require_api_auth")]
    pub required: bool,
    #[serde(default = "default_api_tokens")]
    pub tokens: Vec<String>,
}

impl Default for ApiAuth {
    fn default() -> Self {
        Self {
            required: default_require_api_auth(),
            tokens: default_api_tokens(),
        }
    }
}

impl ApiAuth {
    pub fn generate_token() -> String {
        format!("sk-cluaiz-{}", uuid::Uuid::new_v4().simple())
    }

    pub fn sanitize_tokens(&mut self) {
        self.tokens.retain(|t| {
            let trimmed = t.trim();
            !trimmed.is_empty() && trimmed != "sk-cluaiz-" && trimmed.len() > 10
        });
    }

    pub fn mask_token(token: &str) -> String {
        let trimmed = token.trim();
        if trimmed.len() <= 8 {
            "sk-••••••••".to_string()
        } else {
            let suffix = &trimmed[trimmed.len().saturating_sub(4)..];
            format!("sk-••••{}", suffix)
        }
    }

    pub fn masked_tokens(&self) -> Vec<String> {
        self.tokens.iter().map(|t| Self::mask_token(t)).collect()
    }

    pub fn ensure_valid_token(&mut self) -> Option<String> {
        self.sanitize_tokens();
        if self.required && self.tokens.is_empty() {
            let new_tok = Self::generate_token();
            self.tokens.push(new_tok.clone());
            Some(new_tok)
        } else {
            None
        }
    }

    pub fn remove_token(&mut self, target: &str) -> Result<String, &'static str> {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return Err("Target token identifier cannot be empty");
        }

        if let Ok(idx) = trimmed.parse::<usize>() {
            if idx >= 1 && idx <= self.tokens.len() {
                let removed = self.tokens.remove(idx - 1);
                self.sanitize_tokens();
                return Ok(removed);
            } else {
                return Err("Token index out of range");
            }
        }

        if let Some(pos) = self.tokens.iter().position(|t| t.trim() == trimmed) {
            let removed = self.tokens.remove(pos);
            self.sanitize_tokens();
            return Ok(removed);
        }

        if trimmed.contains('•') || trimmed.contains('*') {
            if let Some((idx, ch)) = trimmed.char_indices().filter(|(_, c)| *c == '•' || *c == '*').last() {
                let suffix = &trimmed[idx + ch.len_utf8()..];
                if suffix.len() < 4 {
                    return Err("Masked token must have at least 4 visible trailing characters");
                }
                let matching: Vec<usize> = self.tokens
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.ends_with(suffix))
                    .map(|(i, _)| i)
                    .collect();

                if matching.is_empty() {
                    return Err("No token matches the provided masked suffix");
                }
                if matching.len() > 1 {
                    return Err("Multiple tokens match this masked suffix. Use index or full token instead.");
                }
                let removed = self.tokens.remove(matching[0]);
                self.sanitize_tokens();
                return Ok(removed);
            }
        }

        Err("Token not found")
    }
}

/// Fields kept only so existing `permission.json` files and the App's settings screen keep
/// working: nothing in the Engine acts on them. CEP `config.get` lists them for the App.
pub const UNUSED_FIELDS: [&str; 6] = [
    "wasm_firewall",
    "vectorize_user_input",
    "vectorize_ai_response",
    "enable_kvcache",
    "vector_models.vision",
    "vector_models.audio",
];

/// One value of `agent_security_mode` and what the Engine does in it. CEP `config.get` sends
/// these, so a settings screen shows the Engine's own rules. The runtime rules are in
/// `ToolEntry::requires_approval` (agents); the install rules in the installer's plan.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SecurityModeInfo {
    pub id: &'static str,
    pub title: &'static str,
    /// What runs without asking and what is asked, in the user's words
    pub summary: &'static str,
    /// The mode a new install starts in
    pub default: bool,
}

pub const SECURITY_MODES: [SecurityModeInfo; 3] = [
    SecurityModeInfo {
        id: "full_access",
        title: "Full Access",
        summary: "Tools run without asking. Still asked: a tool you set to always ask, and each call of a script or MCP tool while one of its permissions is off. At install, a sensitive permission is still asked.",
        default: false,
    },
    SecurityModeInfo {
        id: "sandboxed",
        title: "Sandboxed",
        summary: "Asks before MCP tools, tools that declare no permissions, tools set to confirm, and tools that use the internet, write files or run programs. Tools that only read run without asking, as do tools you set to always allow and the read-only functions of an MCP server whose annotations you trust. At install, a sensitive permission, or a dependency that wants one, is asked.",
        default: true,
    },
    SecurityModeInfo {
        id: "strict",
        title: "Strict",
        summary: "Asks before every tool call, except a tool you set to always allow. At install, every permission is asked.",
        default: false,
    },
];

/// The mode one chat request runs in: the stricter of the global `agent_security_mode` and the
/// request's `approval_mode` (absent = the global mode). Order: full_access < sandboxed < strict.
/// A chat can only tighten: any caller of the API could send the field, so a looser value would let
/// one caller give itself more than the owner set. Loosening is an open owner decision. A value that
/// is not a mode id is an error naming `approval_mode`. Nothing here writes the global setting.
pub fn chat_security_mode(global: &str, requested: Option<&str>) -> Result<&'static str, String> {
    let rank = |id: &str| SECURITY_MODES.iter().position(|m| m.id == id);
    // A global value the Engine does not know runs as sandboxed (as `requires_approval` does)
    let global = rank(&global.to_lowercase()).unwrap_or_else(|| rank("sandboxed").unwrap_or(1));
    let Some(requested) = requested else { return Ok(SECURITY_MODES[global].id) };
    let ids: Vec<&str> = SECURITY_MODES.iter().map(|m| m.id).collect();
    let requested = rank(requested).ok_or_else(|| format!("approval_mode must be one of: {} (got '{requested}')", ids.join(", ")))?;
    Ok(SECURITY_MODES[global.max(requested)].id)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PermissionSchema {
    #[serde(default)]
    pub active_slots: HashMap<String, SlotConfig>,
    #[serde(default)]
    pub vector_models: ModelSelection,
    #[serde(default)]
    pub chat_models: ModelSelection,
    /// Not used: see [`UNUSED_FIELDS`]
    #[serde(default = "default_wasm_firewall")]
    pub wasm_firewall: String,
    /// Not used: see [`UNUSED_FIELDS`]
    #[serde(default = "default_vectorize_user_input")]
    pub vectorize_user_input: bool,
    /// Not used: see [`UNUSED_FIELDS`]
    #[serde(default = "default_vectorize_ai_response")]
    pub vectorize_ai_response: bool,
    #[serde(default = "default_stream_telemetry")]
    pub stream_telemetry: bool,
    #[serde(default = "default_lazy_load_model")]
    pub lazy_load_model: bool,
    /// Not used: see [`UNUSED_FIELDS`]
    #[serde(default = "default_enable_kvcache")]
    pub enable_kvcache: bool,
    #[serde(default = "default_model_header_info")]
    pub model_header_info: bool,
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    #[serde(default = "default_api_host")]
    pub api_host: String,
    #[serde(default = "default_connection_protocol")]
    pub connection_protocol: String,
    #[serde(default)]
    pub api_auth: ApiAuth,
    #[serde(default = "default_agent_security_mode")]
    pub agent_security_mode: String,
    #[serde(default)]
    pub exec: ExecPolicy,
    /// OS confinement of agent commands, MCP servers and native plugins: one of [`OS_SANDBOX_MODES`]
    /// (`agents/src/execution/os_sandbox.rs`). The `CLUAIZ_OS_SANDBOX` environment variable wins for one run.
    #[serde(default = "default_os_sandbox")]
    pub os_sandbox: String,
}

/// `os_sandbox` values: `auto` confines where the platform allows it by default (Linux bubblewrap,
/// macOS sandbox-exec; not Windows), `required` confines or refuses to run, `off` never confines
pub const OS_SANDBOX_MODES: [&str; 3] = ["auto", "required", "off"];

impl Default for PermissionSchema {
    fn default() -> Self {
        Self {
            active_slots: HashMap::new(),
            vector_models: ModelSelection::default(),
            chat_models: ModelSelection::default(),
            wasm_firewall: default_wasm_firewall(),
            vectorize_user_input: default_vectorize_user_input(),
            vectorize_ai_response: default_vectorize_ai_response(),
            stream_telemetry: default_stream_telemetry(),
            lazy_load_model: default_lazy_load_model(),
            enable_kvcache: default_enable_kvcache(),
            model_header_info: default_model_header_info(),
            api_port: default_api_port(),
            api_host: default_api_host(),
            connection_protocol: default_connection_protocol(),
            api_auth: ApiAuth::default(),
            agent_security_mode: default_agent_security_mode(),
            exec: ExecPolicy::default(),
            os_sandbox: default_os_sandbox(),
        }
    }
}

fn default_wasm_firewall() -> String { "auto".to_string() }
fn default_os_sandbox() -> String { "auto".to_string() }
fn default_vectorize_user_input() -> bool { true }
fn default_vectorize_ai_response() -> bool { true }
fn default_stream_telemetry() -> bool { false }
fn default_lazy_load_model() -> bool { true }
fn default_enable_kvcache() -> bool { true }
fn default_model_header_info() -> bool { false }
fn default_require_api_auth() -> bool { false }
fn default_api_tokens() -> Vec<String> { Vec::new() }
fn default_api_port() -> u16 { 8000 }
fn default_agent_security_mode() -> String { "sandboxed".to_string() }

impl PermissionSchema {
    fn file_path() -> PathBuf {
        let env = EnvironmentManager::current();
        let _ = env.ensure_config_dir();
        env.config_dir().join("permission.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::file_path())
    }

    /// A missing file is the defaults. A file that cannot be read or parsed is the defaults too (the
    /// owner's choice of behaviour is unchanged), but it is no longer silent: a warning names the
    /// file and the error, and the broken file is copied to `permission.json.broken` before a later
    /// save can replace it, so its tokens and settings can be recovered.
    pub fn load_from(path: &std::path::Path) -> Self {
        let data = match std::fs::read_to_string(path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => {
                tracing::warn!("permission.json could not be read ({}): {e}. Using the defaults", path.display());
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&data) {
            Ok(schema) => schema,
            Err(e) => {
                let kept = path.with_extension("json.broken");
                let copied = std::fs::write(&kept, &data).is_ok();
                tracing::warn!(
                    "permission.json is not valid ({}): {e}. Using the defaults{}",
                    path.display(),
                    if copied { format!("; the file was copied to {}", kept.display()) } else { String::new() }
                );
                Self::default()
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&Self::file_path())
    }

    /// Writes a temporary file next to the target, then renames it over the target, so a crash
    /// mid-write leaves the old file whole instead of a cut one
    pub fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, data)?;
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }

    pub fn get_active_chat_model(&self) -> Option<String> {
        if let Some(slot) = self.active_slots.get("chat_slot") {
            if slot.model_id.is_some() {
                return slot.model_id.clone();
            }
        }
        self.chat_models.text.clone()
    }

    pub fn get_active_embedding_model(&self) -> Option<String> {
        if let Some(slot) = self.active_slots.get("embed_slot") {
            if slot.model_id.is_some() {
                return slot.model_id.clone();
            }
        }
        self.vector_models.text.clone()
    }

    pub fn get_active_vision_model(&self) -> Option<String> {
        if let Some(slot) = self.active_slots.get("vision_slot") {
            if slot.model_id.is_some() {
                return slot.model_id.clone();
            }
        }
        self.vector_models.vision.clone()
    }

    pub fn get_active_audio_model(&self) -> Option<String> {
        if let Some(slot) = self.active_slots.get("audio_slot") {
            if slot.model_id.is_some() {
                return slot.model_id.clone();
            }
        }
        self.vector_models.audio.clone()
    }

    pub fn set_active_chat_model(model_id: String) {
        let mut schema = Self::load();
        schema.chat_models.text = Some(model_id.clone());
        let slot = schema.active_slots.entry("chat_slot".to_string()).or_default();
        slot.model_id = Some(model_id);
        let _ = schema.save();
    }

    pub fn set_active_embedding_model(model_id: String) {
        let mut schema = Self::load();
        schema.vector_models.text = Some(model_id.clone());
        let slot = schema.active_slots.entry("embed_slot".to_string()).or_default();
        slot.model_id = Some(model_id);
        let _ = schema.save();
    }

    /// Applies the known fields of `patch`. A known field with a wrong value is an error and
    /// changes nothing; unknown fields are ignored. The address fields (`api_port`, `api_host`,
    /// `connection_protocol`) are not changed here.
    pub fn merge_patch(&mut self, patch: &serde_json::Value) -> Result<(), String> {
        let Some(obj) = patch.as_object() else { return Ok(()) };
        // Checked first, so a bad patch changes nothing
        let text = |name: &str, allowed: &[&str]| -> Result<Option<String>, String> {
            match obj.get(name) {
                None => Ok(None),
                Some(value) => value
                    .as_str()
                    .filter(|v| allowed.contains(v))
                    .map(|v| Some(v.to_string()))
                    .ok_or_else(|| format!("{name} must be one of: {}", allowed.join(", "))),
            }
        };
        let flag = |name: &str| -> Result<Option<bool>, String> {
            match obj.get(name) {
                None => Ok(None),
                Some(value) => value.as_bool().map(Some).ok_or_else(|| format!("{name} must be true or false")),
            }
        };
        let modes: Vec<&str> = SECURITY_MODES.iter().map(|m| m.id).collect();
        let mode = text("agent_security_mode", &modes)?;
        let firewall = text("wasm_firewall", &["auto", "strict", "off"])?;
        let os_sandbox = text("os_sandbox", &OS_SANDBOX_MODES)?;
        let flags = [
            ("vectorize_user_input", flag("vectorize_user_input")?),
            ("vectorize_ai_response", flag("vectorize_ai_response")?),
            ("stream_telemetry", flag("stream_telemetry")?),
            ("lazy_load_model", flag("lazy_load_model")?),
            ("enable_kvcache", flag("enable_kvcache")?),
            ("model_header_info", flag("model_header_info")?),
        ];
        if let Some(mode) = mode {
            self.agent_security_mode = mode;
        }
        if let Some(firewall) = firewall {
            self.wasm_firewall = firewall;
        }
        if let Some(os_sandbox) = os_sandbox {
            self.os_sandbox = os_sandbox;
        }
        for (name, value) in flags {
            let Some(value) = value else { continue };
            match name {
                "vectorize_user_input" => self.vectorize_user_input = value,
                "vectorize_ai_response" => self.vectorize_ai_response = value,
                "stream_telemetry" => self.stream_telemetry = value,
                "lazy_load_model" => self.lazy_load_model = value,
                "enable_kvcache" => self.enable_kvcache = value,
                _ => self.model_header_info = value,
            }
        }
        {
            if let Some(auth_val) = obj.get("api_auth") {
                if let Ok(incoming) = serde_json::from_value::<ApiAuth>(auth_val.clone()) {
                    self.api_auth = incoming;
                }
            }
            if let Some(slots) = obj.get("active_slots") {
                if let Ok(parsed) = serde_json::from_value::<HashMap<String, SlotConfig>>(slots.clone()) {
                    self.active_slots = parsed;
                }
            }
            if let Some(cm) = obj.get("chat_models") {
                if let Ok(parsed) = serde_json::from_value::<ModelSelection>(cm.clone()) {
                    self.chat_models = parsed;
                }
            }
            if let Some(exec) = obj.get("exec") {
                if let Ok(parsed) = serde_json::from_value::<ExecPolicy>(exec.clone()) {
                    self.exec = parsed;
                }
            }
            if let Some(vm) = obj.get("vector_models") {
                if let Ok(parsed) = serde_json::from_value::<ModelSelection>(vm.clone()) {
                    self.vector_models = parsed;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cluaiz-perm-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_broken_file_loads_as_defaults_and_is_kept_for_recovery() {
        let dir = temp_dir("broken");
        let path = dir.join("permission.json");
        std::fs::write(&path, "{ \"api_port\": 9000, ").unwrap();
        let schema = PermissionSchema::load_from(&path);
        assert_eq!(schema.api_port, PermissionSchema::default().api_port);
        assert_eq!(std::fs::read_to_string(dir.join("permission.json.broken")).unwrap(), "{ \"api_port\": 9000, ");
        assert_eq!(PermissionSchema::load_from(&dir.join("missing.json")).api_port, PermissionSchema::default().api_port);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn save_replaces_the_file_whole_and_leaves_no_temporary_file() {
        let dir = temp_dir("save");
        let path = dir.join("permission.json");
        std::fs::write(&path, "old").unwrap();
        let mut schema = PermissionSchema::default();
        schema.api_port = 8123;
        schema.save_to(&path).unwrap();
        assert_eq!(PermissionSchema::load_from(&path).api_port, 8123);
        assert!(!dir.join("permission.json.tmp").exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn os_sandbox_patch_takes_only_known_modes() {
        let mut schema = PermissionSchema::default();
        assert_eq!(schema.os_sandbox, "auto");
        schema.merge_patch(&serde_json::json!({"os_sandbox": "required"})).unwrap();
        assert_eq!(schema.os_sandbox, "required");
        assert!(schema.merge_patch(&serde_json::json!({"os_sandbox": "maybe"})).is_err());
        assert_eq!(schema.os_sandbox, "required", "a bad value saves nothing");
    }

    #[test]
    fn the_security_mode_and_switches_are_saved_and_a_wrong_value_changes_nothing() {
        let mut schema = PermissionSchema::default();
        schema.merge_patch(&serde_json::json!({"agent_security_mode": "full_access", "stream_telemetry": true})).unwrap();
        assert_eq!(schema.agent_security_mode, "full_access");
        assert!(schema.stream_telemetry);

        let error = schema.merge_patch(&serde_json::json!({"agent_security_mode": "anything", "model_header_info": true}));
        assert!(error.is_err());
        assert_eq!(schema.agent_security_mode, "full_access", "a bad patch changes nothing");
        assert!(!schema.model_header_info);

        schema.merge_patch(&serde_json::json!({"api_port": 9999, "api_host": "0.0.0.0"})).unwrap();
        assert_eq!(schema.api_port, default_api_port(), "the address is not changed here");
    }

    #[test]
    fn the_listed_modes_are_the_saved_values_and_one_is_the_default() {
        let defaults: Vec<&str> = SECURITY_MODES.iter().filter(|m| m.default).map(|m| m.id).collect();
        assert_eq!(defaults, [default_agent_security_mode()]);
        for mode in SECURITY_MODES {
            let mut schema = PermissionSchema::default();
            schema.merge_patch(&serde_json::json!({ "agent_security_mode": mode.id })).unwrap();
            assert_eq!(schema.agent_security_mode, mode.id);
        }
    }
}

#[cfg(test)]
mod token_and_slot_tests {
    use super::*;

    // Made-up keys in the generated format; none is a real key
    const FIRST: &str = "sk-cluaiz-test0000aaaa1111";
    const SECOND: &str = "sk-cluaiz-test0000bbbb2222";
    const SAME_END: &str = "sk-cluaiz-test0000cccc2222";

    fn auth(tokens: &[&str]) -> ApiAuth {
        ApiAuth { required: false, tokens: tokens.iter().map(|t| t.to_string()).collect() }
    }

    #[test]
    fn remove_token_by_index_counts_from_one() {
        let mut keys = auth(&[FIRST, SECOND]);
        assert_eq!(keys.remove_token(" 2 "), Ok(SECOND.to_string()));
        assert_eq!(keys.tokens, [FIRST]);
        assert_eq!(keys.remove_token("0"), Err("Token index out of range"));
        assert_eq!(keys.remove_token("2"), Err("Token index out of range"));
        assert_eq!(keys.tokens, [FIRST], "a refused index removes nothing");
    }

    #[test]
    fn remove_token_by_the_full_key() {
        let mut keys = auth(&[FIRST, SECOND]);
        assert_eq!(keys.remove_token(&format!("  {FIRST}  ")), Ok(FIRST.to_string()));
        assert_eq!(keys.tokens, [SECOND]);
        assert_eq!(keys.remove_token("sk-cluaiz-not-there-1234"), Err("Token not found"));
    }

    #[test]
    fn remove_token_by_a_masked_suffix() {
        let mut keys = auth(&[FIRST, SECOND]);
        // The form the CLI and the App show, with the bullet or with stars
        assert_eq!(keys.remove_token(&ApiAuth::mask_token(SECOND)), Ok(SECOND.to_string()));
        assert_eq!(keys.remove_token("sk-****1111"), Ok(FIRST.to_string()));
        assert!(keys.tokens.is_empty());
    }

    #[test]
    fn a_masked_suffix_that_is_short_unknown_or_shared_removes_nothing() {
        let mut keys = auth(&[FIRST, SECOND, SAME_END]);
        assert_eq!(keys.remove_token("sk-\u{2022}\u{2022}\u{2022}\u{2022}222"), Err("Masked token must have at least 4 visible trailing characters"));
        assert_eq!(keys.remove_token("sk-****9999"), Err("No token matches the provided masked suffix"));
        assert_eq!(
            keys.remove_token("sk-****2222"),
            Err("Multiple tokens match this masked suffix. Use index or full token instead.")
        );
        assert_eq!(keys.tokens.len(), 3);
        assert_eq!(keys.remove_token("   "), Err("Target token identifier cannot be empty"));
    }

    #[test]
    fn removing_a_key_also_drops_keys_that_are_not_valid() {
        // Today's behaviour: `remove_token` runs `sanitize_tokens` after a removal
        let mut keys = auth(&[FIRST, "short", SECOND]);
        assert_eq!(keys.remove_token(FIRST), Ok(FIRST.to_string()));
        assert_eq!(keys.tokens, [SECOND]);
    }

    #[test]
    fn each_slot_getter_takes_the_slot_model_first_then_the_older_field() {
        let mut schema = PermissionSchema::default();
        schema.active_slots.clear();
        schema.chat_models = ModelSelection { text: Some("chat-old".into()), vision: None, audio: None };
        schema.vector_models = ModelSelection {
            text: Some("embed-old".into()),
            vision: Some("vision-old".into()),
            audio: Some("audio-old".into()),
        };
        // No slot: the older field
        assert_eq!(schema.get_active_chat_model().as_deref(), Some("chat-old"));
        assert_eq!(schema.get_active_embedding_model().as_deref(), Some("embed-old"));
        assert_eq!(schema.get_active_vision_model().as_deref(), Some("vision-old"));
        assert_eq!(schema.get_active_audio_model().as_deref(), Some("audio-old"));

        // A slot without a model: still the older field
        for slot in ["chat_slot", "embed_slot", "vision_slot", "audio_slot"] {
            schema.active_slots.insert(slot.into(), SlotConfig::default());
        }
        assert_eq!(schema.get_active_chat_model().as_deref(), Some("chat-old"));
        assert_eq!(schema.get_active_audio_model().as_deref(), Some("audio-old"));

        // A slot with a model wins
        for (slot, model) in [("chat_slot", "chat-new"), ("embed_slot", "embed-new"), ("vision_slot", "vision-new"), ("audio_slot", "audio-new")] {
            schema.active_slots.insert(slot.into(), SlotConfig { model_id: Some(model.into()), ..Default::default() });
        }
        assert_eq!(schema.get_active_chat_model().as_deref(), Some("chat-new"));
        assert_eq!(schema.get_active_embedding_model().as_deref(), Some("embed-new"));
        assert_eq!(schema.get_active_vision_model().as_deref(), Some("vision-new"));
        assert_eq!(schema.get_active_audio_model().as_deref(), Some("audio-new"));

        // Neither: none
        let mut empty = PermissionSchema::default();
        empty.active_slots.clear();
        empty.chat_models = ModelSelection::default();
        assert_eq!(empty.get_active_chat_model(), None);
    }

    #[test]
    fn chat_mode_only_tightens() {
        assert_eq!(chat_security_mode("full_access", None), Ok("full_access"));
        assert_eq!(chat_security_mode("full_access", Some("strict")), Ok("strict"));
        assert_eq!(chat_security_mode("full_access", Some("sandboxed")), Ok("sandboxed"));
        assert_eq!(chat_security_mode("sandboxed", Some("full_access")), Ok("sandboxed"), "never looser than global");
        assert_eq!(chat_security_mode("strict", Some("full_access")), Ok("strict"));
        assert_eq!(chat_security_mode("STRICT", None), Ok("strict"));
        assert_eq!(chat_security_mode("unknown", None), Ok("sandboxed"));
        assert_eq!(chat_security_mode("unknown", Some("full_access")), Ok("sandboxed"));
        let error = chat_security_mode("sandboxed", Some("yolo")).unwrap_err();
        assert!(error.contains("approval_mode") && error.contains("strict"), "{error}");
    }
}
