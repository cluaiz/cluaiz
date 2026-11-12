//! Calls to model providers (`config/providers.json`, `cluaiz_core::providers`), shared by the
//! terminal commands, the CEP `providers.*` methods and `/v1/chat/completions`. One job per file:
//!
//! | File | Job |
//! |---|---|
//! | `http.rs` | The client, and the key the way each provider takes it (Bearer, `api-key`, `x-api-key`, a Google or Entra token, an AWS signature) |
//! | `google.rs` | A Google Cloud access token for Vertex AI (service account, user login, gcloud) |
//! | `aws.rs` | AWS credentials and Signature Version 4 for Amazon Bedrock without an API key |
//! | `azure.rs` | A Microsoft Entra ID token for Azure without an API key (service principal, Azure CLI) |
//! | `anthropic.rs` | The Anthropic Messages format: Claude on the Claude API, Amazon Bedrock and Vertex AI |
//! | `compat.rs` | How each provider differs from the OpenAI format: told from its URL, set in providers.json |
//! | `request.rs` | The chat request: samplers, answer limit, thinking on/off and level, tools |
//! | `history.rs` | The Engine's conversation as the provider's messages |
//! | `stream.rs` | The streamed answer as the Engine's thinking, text and `<tool_call>` blocks |
//! | `errors.rs` | Failures in plain words, and their kind |
//! | `retry.rs` | When a request is sent again |
//! | `chat.rs` | One chat request from start to end (`open`, `ProviderRun`) |
//! | `models.rs` | The models a provider offers, and `provider test` |
//! | `media/` | Embeddings, speech and transcription |
//!
//! The key (or sign-in token) goes only into the auth header of a request to the provider's own URL. Every
//! message that could hold it passes through `redact` first.

mod anthropic;
mod aws;
mod azure;
mod chat;
mod compat;
mod errors;
mod google;
mod history;
mod http;
mod media;
mod models;
mod request;
mod retry;
mod stream;
#[cfg(test)]
mod tests;

pub use chat::{open, Opened, Opening, ProviderRun, Room};
pub use compat::{chat_url, Compat, IdForm, ThinkingFormat, Wire};
pub use errors::{classify, error_message, send_error, Failure, CONTEXT_FULL};
pub use history::{drop_oldest_turn, openai_conversation, openai_conversation_with_parts};
pub use http::{client, request, Auth, ANTHROPIC_VERSION};
pub use media::{forward_embeddings, forward_speech, forward_transcription};
pub use models::{models, test, OfferedModel};
pub use request::{chat_body, ChatParts, Effort, Sampling, ThinkMode, Thinking};
pub use stream::{
    data_of, delta_reasoning, event_usage, sse_lines, Decoder, Finished, Read, StreamedCalls,
    THINK_END, THINK_START,
};

use cluaiz_core::providers::Target;
use serde_json::{json, Value};

/// One sentence when providers are set up but API authentication is off: anyone who can reach the
/// server can then use the providers' keys. The owner decides; the Engine only says so.
pub fn open_server_warning(auth_required: bool) -> Option<String> {
    if auth_required {
        return None;
    }
    let providers = cluaiz_core::providers::Providers::load_or_empty();
    let using_keys = providers
        .providers
        .values()
        .any(|p| p.api_key_env.is_some() || p.api_key.is_some());
    using_keys.then(|| {
        "API authentication is off: anyone who can reach this server can use your provider keys. That is fine on your own computer; on a shared network or a server, turn it on with `cluaiz token create`.".to_string()
    })
}

/// The provider model a chat request names (see `target_for_model`)
pub fn target_of(request: &Value) -> Option<Target> {
    target_for_model(request.get("model").and_then(Value::as_str))
}

/// A provider's model named in a CEP call that works on installed models only: a clear error
/// instead of "not found"
pub fn refuse_provider_model(model: &str, what: &str) -> Result<(), cluaiz_cep::CepError> {
    let Some(target) = target_of(&json!({ "model": model })) else {
        return Ok(());
    };
    Err(cluaiz_cep::CepError::conflict(
        "provider_model",
        format!("{} runs at the provider {}: {what} works on installed models only. Use it in /v1/chat/completions; providers.* manages it", target.full_name(), target.provider),
    ))
}

/// The provider model a request names in `model` (no default: embeddings and audio without a
/// model use the installed default, as before). None when it is not a provider's model.
pub fn named_target(request: &Value) -> Option<Target> {
    let name = request
        .get("model")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty() && *m != "auto")?;
    if !name.contains('/') || !crate::providers_configured() {
        return None;
    }
    target_of(request)
}

/// The provider's model a chat names: `model`, or for no model or "auto" the default chat model
/// when that is a provider's. An installed model of the same name wins. None without providers.
pub fn target_for_model(model: Option<&str>) -> Option<Target> {
    let providers = cluaiz_core::providers::Providers::load_or_empty();
    if providers.providers.is_empty() {
        return None;
    }
    let installed = |name: &str| {
        cluaiz_models::registry::InstalledStateRegistry::load()
            .installed_models
            .contains_key(name)
    };
    let name = match model.filter(|m| !m.trim().is_empty() && *m != "auto") {
        Some(name) => name.to_string(),
        None => cluaiz_security::PermissionSchema::load().get_active_chat_model()?,
    };
    providers.find(&name).filter(|_| !installed(&name))
}
