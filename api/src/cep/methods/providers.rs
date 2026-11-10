//! `providers.*`: the model providers in providers.json (services with an OpenAI-compatible API),
//! whose models a chat request names as `<provider>/<model>`. Replies never hold a key: only where
//! it comes from (`env MYAPI_KEY`, `file sk-a…a1b2`). API authentication stays the owner's
//! setting; these methods only add a `warning` when it is off and keys are set up.

use std::collections::BTreeMap;

use crate::cep::CallCtx;
use cluaiz_cep::{Call, CepError, Meta, NoParams, Registry, Scope};
use cluaiz_core::providers::{self, Provider, ProviderModel, Providers, Target};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetParams {
    /// Provider name, the first part of its model names (`myapi` → `myapi/<model>`): letters, digits, - and _
    name: String,
    /// OpenAI-compatible base URL, such as `https://api.example.com/v1`
    base_url: String,
    /// Name of the environment variable that holds the key, such as `MYAPI_KEY`
    #[serde(default)]
    api_key_env: Option<String>,
    /// The key itself, kept in providers.json. Leave both out to keep the current key
    #[serde(default)]
    api_key: Option<String>,
    /// Models to list, with their context size in tokens: `{"model-name": {"context": 32768}}`
    #[serde(default)]
    models: Option<BTreeMap<String, ModelParams>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ModelParams {
    /// Context size in tokens
    #[serde(default)]
    context: Option<u32>,
    /// What the model does: "chat" (default), "embedding", "tts" or "stt"
    #[serde(default)]
    kind: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NameParams {
    /// Provider name
    name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TestParams {
    /// Provider name
    name: String,
    /// The model to try; default: the first listed one
    #[serde(default)]
    model: Option<String>,
    /// For a text-to-speech model: the voice, when the provider needs one
    #[serde(default)]
    voice: Option<String>,
}

fn load() -> Result<Providers, CepError> {
    Providers::load().map_err(|e| CepError::unavailable("providers_unreadable", format!("providers.json could not be read: {e}")))
}

fn save(all: &Providers) -> Result<(), CepError> {
    all.save().map_err(|e| CepError::unavailable("settings_not_saved", format!("providers.json could not be saved: {e}")))
}

fn find<'a>(all: &'a Providers, name: &str) -> Result<&'a Provider, CepError> {
    all.providers.get(name).ok_or_else(|| CepError::not_found("provider_not_found", format!("No provider '{name}' (see providers.list)")))
}

fn shown(name: &str, provider: &Provider) -> Value {
    let models: Vec<Value> = provider
        .models
        .iter()
        .map(|(id, info)| json!({ "id": id, "model": format!("{name}/{id}"), "kind": info.kind.as_deref().unwrap_or("chat"), "context": info.context.unwrap_or(providers::DEFAULT_CONTEXT) }))
        .collect();
    json!({ "name": name, "type": provider.kind, "base_url": provider.base_url, "key": provider.key_source(), "models": models })
}

/// The reply, with `warning` when API authentication is off and a provider has a key
async fn reply(ctx: &CallCtx, mut value: Value) -> Value {
    let required = ctx.state.permission_schema.read().await.api_auth.required;
    if let Some(warning) = crate::providers::open_server_warning(required) {
        value["warning"] = json!(warning);
    }
    value
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry
        .add(
            "providers.list",
            Meta::new(Scope::Admin, "The model providers and their listed models; keys are never returned").no_mcp().example(
                json!({}),
                json!({ "providers": [{ "name": "myapi", "type": "openai", "base_url": "https://api.example.com/v1", "key": "env MYAPI_KEY", "models": [{ "id": "model-name", "model": "myapi/model-name", "kind": "chat", "context": 32768 }] }] }),
            ),
            |ctx: CallCtx, _: Call, _: NoParams| async move {
                let all = load()?;
                let list: Vec<Value> = all.providers.iter().map(|(name, p)| shown(name, p)).collect();
                Ok(reply(&ctx, json!({ "providers": list })).await)
            },
        )
        .add(
            "providers.set",
            Meta::new(Scope::Admin, "Adds a provider, or changes its URL, key source and listed models")
                .no_mcp()
                .error(-32602, "invalid_params", "The name, the URL or a model's kind is not valid, or both api_key and api_key_env were sent")
                .error(-32010, "settings_not_saved", "providers.json could not be written")
                .example(
                    json!({ "name": "myapi", "base_url": "https://api.example.com/v1", "api_key_env": "MYAPI_KEY", "models": { "model-name": { "context": 32768 } } }),
                    json!({ "provider": { "name": "myapi", "type": "openai", "base_url": "https://api.example.com/v1", "key": "env MYAPI_KEY", "models": [{ "id": "model-name", "model": "myapi/model-name", "kind": "chat", "context": 32768 }] } }),
                ),
            |ctx: CallCtx, _: Call, p: SetParams| async move {
                if !providers::valid_name(&p.name) {
                    return Err(CepError::invalid_params(format!("'{}': a provider name has letters, digits, - and _ only", p.name), vec![], None));
                }
                let url = p.base_url.trim();
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(CepError::invalid_params(format!("'{url}': base_url starts with http:// or https://"), vec![], None));
                }
                if p.api_key.is_some() && p.api_key_env.is_some() {
                    return Err(CepError::invalid_params("Send api_key or api_key_env, not both", vec![], None));
                }
                let mut all = load()?;
                let mut provider = all.providers.get(&p.name).cloned().unwrap_or_else(|| Provider::new(url));
                provider.base_url = url.trim_end_matches('/').to_string();
                if let Some(var) = p.api_key_env {
                    provider.api_key_env = Some(var).filter(|v| !v.is_empty());
                    provider.api_key = None;
                }
                if let Some(key) = p.api_key {
                    provider.api_key = Some(key).filter(|k| !k.is_empty());
                    provider.api_key_env = None;
                }
                if let Some(models) = p.models {
                    if let Some((id, kind)) = models.iter().find_map(|(id, m)| m.kind.as_deref().filter(|k| !providers::valid_kind(k)).map(|k| (id.clone(), k.to_string()))) {
                        return Err(CepError::invalid_params(format!("{id}: kind '{kind}' is not chat, embedding, tts or stt"), vec![], None));
                    }
                    provider.models = models.into_iter().map(|(id, m)| (id, ProviderModel { context: m.context, kind: m.kind })).collect();
                }
                all.providers.insert(p.name.clone(), provider.clone());
                save(&all)?;
                Ok(reply(&ctx, json!({ "provider": shown(&p.name, &provider) })).await)
            },
        )
        .add(
            "providers.remove",
            Meta::new(Scope::Admin, "Removes a provider, with its key if it was kept in the file")
                .no_mcp()
                .destructive()
                .error(-32004, "provider_not_found", "No provider has this name")
                .error(-32010, "settings_not_saved", "providers.json could not be written")
                .example(json!({ "name": "myapi" }), json!({ "removed": "myapi" })),
            |_ctx: CallCtx, _: Call, p: NameParams| async move {
                let mut all = load()?;
                find(&all, &p.name)?;
                all.providers.remove(&p.name);
                save(&all)?;
                Ok(json!({ "removed": p.name }))
            },
        )
        .add(
            "providers.models",
            Meta::new(Scope::Admin, "The models a provider offers (its GET /models), with the context size it reports")
                .no_mcp()
                .error(-32004, "provider_not_found", "No provider has this name")
                .error(-32010, "provider_unavailable", "The provider did not answer, or refused the key")
                .example(
                    json!({ "name": "myapi" }),
                    json!({ "models": [{ "id": "model-name", "model": "myapi/model-name", "context": 32768 }] }),
                ),
            |_ctx: CallCtx, _: Call, p: NameParams| async move {
                let all = load()?;
                let provider = find(&all, &p.name)?;
                let offered = crate::providers::models(&p.name, provider).await.map_err(|e| CepError::unavailable("provider_unavailable", e))?;
                let models: Vec<Value> = offered.into_iter().map(|m| json!({ "model": format!("{}/{}", p.name, m.id), "id": m.id, "context": m.context })).collect();
                Ok(json!({ "models": models }))
            },
        )
        .add(
            "providers.test",
            Meta::new(Scope::Admin, "Sends one short request of the model's kind: the URL, the key and the model work")
                .no_mcp()
                .error(-32004, "provider_not_found", "No provider has this name")
                .error(-32602, "invalid_params", "No model was sent and the provider lists none")
                .error(-32010, "provider_unavailable", "The provider did not answer, or refused the request")
                .example(
                    json!({ "name": "myapi", "model": "model-name" }),
                    json!({ "model": "myapi/model-name", "answer": "ready", "ms": 420 }),
                ),
            |_ctx: CallCtx, _: Call, p: TestParams| async move {
                let all = load()?;
                let provider = find(&all, &p.name)?;
                let model = p
                    .model
                    .or_else(|| provider.models.keys().next().cloned())
                    .ok_or_else(|| CepError::invalid_params("Send `model`: this provider lists no models (see providers.models)", vec![], None))?;
                let target = Target { provider: p.name.clone(), config: provider.clone(), model };
                let started = std::time::Instant::now();
                let answer = crate::providers::test(&target, p.voice.as_deref()).await.map_err(|e| CepError::unavailable("provider_unavailable", e))?;
                Ok(json!({ "model": target.full_name(), "answer": answer, "ms": started.elapsed().as_millis() as u64 }))
            },
        );
}
