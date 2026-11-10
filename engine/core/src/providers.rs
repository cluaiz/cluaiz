//! Model providers: other services with an OpenAI-compatible API that the Engine can send a chat
//! to. `config/providers.json` lists them; a model is named `<provider>/<model>`, such as
//! `myapi/model-name`. Every provider speaks the OpenAI Chat Completions format for now.
//!
//! A key is read from the environment variable named in `api_key_env` (the Engine also reads
//! `config/.env`), or from `api_key` in the file. Keys are never logged or shown in full.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The only request format so far: OpenAI Chat Completions (`POST {base_url}/chat/completions`)
pub const OPENAI: &str = "openai";

/// Context used when neither the file nor the provider names one
pub const DEFAULT_CONTEXT: u32 = 32_768;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Providers {
    #[serde(default)]
    pub providers: BTreeMap<String, Provider>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    /// Request format; only "openai" so far
    #[serde(rename = "type", default = "openai_type")]
    pub kind: String,
    /// Such as `https://api.example.com/v1`; the Engine adds `/chat/completions` and `/models`
    pub base_url: String,
    /// Name of the environment variable that holds the key, such as `MYAPI_KEY`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// The key itself, when it is kept in this file
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Models with their context size. Other models of the provider can be used too.
    #[serde(default)]
    pub models: BTreeMap<String, ProviderModel>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProviderModel {
    /// Context size in tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<u32>,
    /// What the model does, as the Engine's model categories: "chat" (the default), "embedding",
    /// "tts" (text to speech) or "stt" (speech to text)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// The kinds a provider's model can have, as the Engine's model categories, with the route each uses
pub const KINDS: [(&str, &str); 4] = [
    ("chat", "/v1/chat/completions"),
    ("embedding", "/v1/embeddings"),
    ("tts", "/v1/audio/speech"),
    ("stt", "/v1/audio/transcriptions"),
];

/// `kind` when it is one of `KINDS`
pub fn valid_kind(kind: &str) -> bool {
    KINDS.iter().any(|(k, _)| *k == kind)
}

fn openai_type() -> String {
    OPENAI.to_string()
}

/// A model of a provider, chosen by its full name
#[derive(Debug, Clone)]
pub struct Target {
    /// The provider's name in providers.json
    pub provider: String,
    pub config: Provider,
    /// The model id the provider knows, such as `qwen/qwen3-32b`
    pub model: String,
}

impl Target {
    /// `<provider>/<model>`
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }

    /// The context size of this model: from the file, else the default
    pub fn context(&self) -> u32 {
        self.config.models.get(&self.model).and_then(|m| m.context).unwrap_or(DEFAULT_CONTEXT)
    }

    /// The kind providers.json gives this model; None when it is not listed or has none
    pub fn listed_kind(&self) -> Option<&str> {
        self.config.models.get(&self.model).and_then(|m| m.kind.as_deref())
    }

    /// An error when providers.json lists this model as another kind than `wanted`: such as an
    /// embedding model sent to the chat route. A model without a kind fits every route.
    pub fn check_kind(&self, wanted: &str) -> Result<(), String> {
        match self.listed_kind() {
            Some(kind) if kind != wanted => {
                let route = KINDS.iter().find(|(k, _)| *k == kind).map(|(_, r)| *r).unwrap_or("its own route");
                Err(format!("{} is listed with kind \"{kind}\" in providers.json: use it with {route}", self.full_name()))
            }
            _ => Ok(()),
        }
    }
}

impl Providers {
    pub fn path() -> PathBuf {
        crate::EnvironmentManager::current().config_dir().join("providers.json")
    }

    /// The file, or no providers when it is missing; an unreadable file is an error
    pub fn load() -> Result<Self, String> {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) if text.trim().is_empty() => Ok(Self::default()),
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// The file, or no providers when it cannot be read (logged once per call)
    pub fn load_or_empty() -> Self {
        Self::load().unwrap_or_else(|error| {
            tracing::warn!("providers.json ignored: {error}");
            Self::default()
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        // A key may be in the file: only its owner reads it
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// `<provider>/<model>` when `<provider>` is in the file and a model follows the slash
    pub fn find(&self, name: &str) -> Option<Target> {
        let (provider, model) = name.split_once('/')?;
        let config = self.providers.get(provider)?;
        let model = model.trim();
        if model.is_empty() {
            return None;
        }
        Some(Target { provider: provider.to_string(), config: config.clone(), model: model.to_string() })
    }

    /// Every listed model as its full name, with its context size and kind ("chat" when none)
    pub fn listed_models(&self) -> Vec<(String, u32, String)> {
        self.providers
            .iter()
            .flat_map(|(name, provider)| {
                provider.models.iter().map(move |(model, info)| {
                    (format!("{name}/{model}"), info.context.unwrap_or(DEFAULT_CONTEXT), info.kind.clone().unwrap_or_else(|| "chat".into()))
                })
            })
            .collect()
    }
}

/// A provider name: letters, digits, `-` and `_` (it is the first part of a model name)
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Provider {
    pub fn new(base_url: &str) -> Self {
        Self { kind: OPENAI.to_string(), base_url: base_url.trim().trim_end_matches('/').to_string(), api_key_env: None, api_key: None, models: BTreeMap::new() }
    }

    /// `{base_url}/chat/completions`
    pub fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// `{base_url}/models`
    pub fn models_url(&self) -> String {
        format!("{}/models", self.base_url.trim_end_matches('/'))
    }

    /// The key: from the environment variable when one is named, else from the file. None when
    /// the provider needs none (a server on this computer). Err names the missing variable.
    pub fn key(&self) -> Result<Option<String>, String> {
        if let Some(var) = self.api_key_env.as_deref().filter(|v| !v.is_empty()) {
            return match env_value(var) {
                Some(value) => Ok(Some(value)),
                None => Err(format!(
                    "the environment variable {var} is not set: set it, or add `{var}=<key>` to {}",
                    crate::EnvironmentManager::current().env_file_path().display()
                )),
            };
        }
        Ok(self.api_key.clone().filter(|k| !k.trim().is_empty()))
    }

    /// Where the key comes from, without the key: "env MYAPI_KEY", "file sk-a…a1b2" or "none"
    pub fn key_source(&self) -> String {
        if let Some(var) = self.api_key_env.as_deref().filter(|v| !v.is_empty()) {
            let set = env_value(var).is_some();
            return format!("env {var}{}", if set { "" } else { " (not set)" });
        }
        match self.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            Some(key) => format!("file {}", mask(key)),
            None => "none".to_string(),
        }
    }
}

/// A variable from the environment, else from `config/.env` of the Cluaiz folder in use (also when
/// CLUAIZ_HOME names it)
fn env_value(var: &str) -> Option<String> {
    if let Some(value) = std::env::var(var).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
        return Some(value);
    }
    let text = std::fs::read_to_string(crate::EnvironmentManager::current().env_file_path()).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.trim().split_once('=')?;
        let value = value.trim().trim_matches('"').trim_matches('\'');
        (key.trim() == var && !value.is_empty()).then(|| value.to_string())
    })
}

/// A key as people may see it: the first 4 and last 4 characters
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 12 {
        return format!("…{}", chars[chars.len().saturating_sub(2)..].iter().collect::<String>());
    }
    format!("{}…{}", chars[..4].iter().collect::<String>(), chars[chars.len() - 4..].iter().collect::<String>())
}

/// `text` with every copy of `key` replaced by its masked form (for error messages)
pub fn redact(text: &str, key: Option<&str>) -> String {
    match key.filter(|k| k.len() >= 8) {
        Some(key) => text.replace(key, &mask(key)),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Providers {
        let mut providers = Providers::default();
        let mut groq = Provider::new("https://api.groq.com/openai/v1/");
        groq.api_key_env = Some("CLUAIZ_TEST_NO_SUCH_KEY".into());
        groq.models.insert("qwen/qwen3-32b".into(), ProviderModel { context: Some(131_072), kind: None });
        providers.providers.insert("groq".into(), groq);
        providers.providers.insert("ollama".into(), Provider::new("http://localhost:11434/v1"));
        providers
    }

    #[test]
    fn a_full_name_finds_the_provider_and_keeps_the_rest_as_the_model() {
        let providers = sample();
        let target = providers.find("groq/qwen/qwen3-32b").unwrap();
        assert_eq!(target.provider, "groq");
        assert_eq!(target.model, "qwen/qwen3-32b");
        assert_eq!(target.context(), 131_072);
        assert_eq!(target.config.chat_url(), "https://api.groq.com/openai/v1/chat/completions");
        // A model the file does not list still works, with the default context
        assert_eq!(providers.find("ollama/qwen3:8b").unwrap().context(), DEFAULT_CONTEXT);
        // A kind is checked only when the file gives one
        assert!(target.check_kind("embedding").is_ok());
        let mut whisper = providers.find("groq/whisper-large-v3").unwrap();
        whisper.config.models.insert("whisper-large-v3".into(), ProviderModel { context: None, kind: Some("stt".into()) });
        assert!(whisper.check_kind("stt").is_ok());
        assert!(whisper.check_kind("chat").unwrap_err().contains("/v1/audio/transcriptions"));
        assert!(valid_kind("tts") && !valid_kind("speech"));
        // Not a provider: a local or Hugging Face id
        assert!(providers.find("unsloth/Qwen3-4B-GGUF").is_none());
        assert!(providers.find("groq/").is_none());
        assert!(providers.find("qwen3-4b").is_none());
    }

    #[test]
    fn the_file_round_trips_and_type_defaults_to_openai() {
        let text = r#"{"providers":{"groq":{"base_url":"https://api.groq.com/openai/v1","api_key_env":"GROQ_API_KEY"}}}"#;
        let providers: Providers = serde_json::from_str(text).unwrap();
        assert_eq!(providers.providers["groq"].kind, OPENAI);
        let back: Providers = serde_json::from_str(&serde_json::to_string(&providers).unwrap()).unwrap();
        assert_eq!(back.providers["groq"], providers.providers["groq"]);
    }

    #[test]
    fn keys_are_never_shown_in_full() {
        let key = "gsk_abcdefghijklmnopqrstuvwxyz123456";
        assert_eq!(mask(key), "gsk_…3456");
        assert!(!redact(&format!("bad key {key}"), Some(key)).contains("abcdefgh"));
        let mut provider = Provider::new("https://x/v1");
        provider.api_key = Some(key.into());
        assert_eq!(provider.key_source(), "file gsk_…3456");
        assert_eq!(provider.key().unwrap().as_deref(), Some(key));
    }

    #[test]
    fn a_missing_variable_is_named() {
        let providers = sample();
        let error = providers.providers["groq"].key().unwrap_err();
        assert!(error.contains("CLUAIZ_TEST_NO_SUCH_KEY"));
        assert_eq!(providers.providers["ollama"].key().unwrap(), None);
        assert!(valid_name("open-router_1") && !valid_name("a/b") && !valid_name(""));
    }
}
