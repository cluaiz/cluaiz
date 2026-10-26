//! `tokens.*`: the API keys in permission.json. Keys and auth on/off are the owner's settings;
//! these methods never turn auth on or off.

use crate::cep::CallCtx;
use cluaiz_cep::{Call, CepError, Meta, NoParams, Registry, Scope};
use cluaiz_security::permission_schema::ApiAuth;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

/// At most this many API keys at once
const MAX_TOKENS: usize = 5;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Revoke {
    /// The key itself, or its masked form (at least 4 visible trailing characters)
    #[serde(default)]
    token: Option<String>,
    /// Or its position in `tokens.list`, from 1
    #[serde(default)]
    index: Option<usize>,
}

fn keys(auth: &ApiAuth) -> Value {
    json!({ "tokens": auth.tokens, "api_auth_required": auth.required })
}

fn save_error(e: impl std::fmt::Display) -> CepError {
    CepError::unavailable("settings_not_saved", format!("permission.json could not be saved: {e}"))
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry
        .add(
            "tokens.list",
            Meta::new(Scope::Admin, "The API keys, and whether API authentication is required").no_mcp().example(
                json!({}),
                json!({ "tokens": ["sk-cluaiz-1f0c…"], "api_auth_required": false }),
            ),
            |ctx: CallCtx, _: Call, _: NoParams| async move { Ok(keys(&ctx.state.permission_schema.read().await.api_auth)) },
        )
        .add(
            "tokens.create",
            Meta::new(Scope::Admin, "Makes a new API key (at most 5)")
                .no_mcp()
                .idempotent(false)
                .error(-32005, "token_limit", "There are 5 keys already: revoke one first")
                .error(-32010, "settings_not_saved", "permission.json could not be written")
                .example(
                json!({}),
                json!({ "token": "sk-cluaiz-1f0c…", "tokens": ["sk-cluaiz-1f0c…"], "api_auth_required": false }),
            ),
            |ctx: CallCtx, _: Call, _: NoParams| async move {
                let mut schema = ctx.state.permission_schema.read().await.clone();
                if schema.api_auth.tokens.len() >= MAX_TOKENS {
                    return Err(CepError::conflict("token_limit", format!("At most {MAX_TOKENS} API keys; revoke one first")));
                }
                let token = ApiAuth::generate_token();
                schema.api_auth.tokens.push(token.clone());
                schema.api_auth.sanitize_tokens();
                schema.save().map_err(save_error)?;
                let mut reply = keys(&schema.api_auth);
                reply["token"] = json!(token);
                *ctx.state.permission_schema.write().await = schema;
                Ok(reply)
            },
        )
        .add(
            "tokens.revoke",
            Meta::new(Scope::Admin, "Removes an API key; with none left, API authentication turns off (as before)")
                .no_mcp()
                .destructive()
                .error(-32602, "invalid_params", "Neither `token` nor `index` was sent")
                .error(-32004, "token_not_found", "No key matches `token`, or `index` is out of range")
                .error(-32010, "settings_not_saved", "permission.json could not be written")
                .example(json!({ "index": 1 }), json!({ "revoked_token": "sk-cluaiz-1f0c…", "tokens": [], "api_auth_required": false })),
            |ctx: CallCtx, _: Call, p: Revoke| async move {
                let target = match (p.index, p.token) {
                    (Some(index), _) => index.to_string(),
                    (None, Some(token)) => token.trim().to_string(),
                    (None, None) => return Err(CepError::invalid_params("Send `token` or `index`", Vec::new(), Some(json!({ "index": 1 })))),
                };
                let mut schema = ctx.state.permission_schema.read().await.clone();
                let revoked = schema.api_auth.remove_token(&target).map_err(|e| CepError::not_found("token_not_found", e))?;
                if schema.api_auth.tokens.is_empty() {
                    // No keys left would lock every client out
                    schema.api_auth.required = false;
                    tracing::warn!("⚠️ All API tokens revoked: Automatically disabled api_auth.required to prevent engine lockout.");
                }
                schema.save().map_err(save_error)?;
                let mut reply = keys(&schema.api_auth);
                reply["revoked_token"] = json!(revoked);
                *ctx.state.permission_schema.write().await = schema;
                Ok(reply)
            },
        );
}
