//! `approvals.*`: the owner's answers to approval requests (03-method-catalog Part B). One store
//! for everything that asks: a chat tool call waiting in the agent loop, and a CEP call outside
//! its caller's roots. Only the owner answers. Owner: Claude (Wave A).

use crate::cep::CallCtx;
use crate::handlers::chat::authorize;
use cluaiz_cep::{Call, Meta, NoParams, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Approve {
    approval_id: String,
    /// "chat": also allow this tool in this chat without asking again, for `minutes`
    #[serde(default)]
    remember: Option<String>,
    /// How long a "chat" grant lasts (default 60, at most 1440)
    #[serde(default)]
    minutes: Option<u64>,
    /// Passed to the waiting call
    #[serde(default)]
    feedback: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Reject {
    approval_id: String,
    /// Passed to the waiting call (the model sees it as the reason)
    #[serde(default)]
    feedback: Option<String>,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    let approval = json!({ "id": "ap_41c2…", "principal": "agent", "method": "mcp:web-search", "target": "{\"query\":\"news\"}",
                           "created_at": "2026-10-25T10:30:00Z", "expires_at": "2026-10-25T10:35:00Z", "status": "pending" });
    registry
        .add(
            "approvals.list",
            Meta::new(Scope::Admin, "Approval requests waiting for the owner").owner_only().example(json!({}), json!({ "pending": [approval] })),
            |ctx: CallCtx, _: Call, _: NoParams| async move { Ok(json!({ "pending": ctx.state.cep.shared().approvals.pending() })) },
        )
        .add(
            "approvals.approve",
            Meta::new(Scope::Admin, "Allows the waiting call; with remember chat, the tool also runs without asking in that chat for a while")
                .owner_only()
                .example(json!({ "approval_id": "ap_41c2…", "remember": "chat", "minutes": 60 }), json!({ "approval": approval, "grant": { "chat_id": "c1", "tool": "web-search", "minutes": 60 } })),
            |ctx: CallCtx, _: Call, p: Approve| async move {
                let grant = match p.remember.as_deref() {
                    Some("chat") => authorize::grant_from_request(&p.approval_id, p.minutes),
                    Some(other) => return Err(cluaiz_cep::CepError::invalid_params(format!("`remember` is \"chat\", not \"{other}\""), Vec::new(), None)),
                    None => None,
                };
                let approval = ctx.state.cep.shared().approvals.decide(&p.approval_id, true, p.feedback)?;
                if let Some((chat, tool, minutes)) = &grant {
                    let event = cluaiz_agents::memory::NewEvent::new("grant", json!({ "tool": tool, "minutes": minutes, "scope": "chat" }));
                    let _ = cluaiz_agents::memory::store_for(chat).append(chat, vec![event]);
                }
                Ok(json!({ "approval": approval, "grant": grant.map(|(chat, tool, minutes)| json!({ "chat_id": chat, "tool": tool, "minutes": minutes })) }))
            },
        )
        .add(
            "approvals.reject",
            Meta::new(Scope::Admin, "Refuses the waiting call").owner_only().example(
                json!({ "approval_id": "ap_41c2…", "feedback": "Not this site" }),
                json!({ "approval": approval }),
            ),
            |ctx: CallCtx, _: Call, p: Reject| async move {
                let approval = ctx.state.cep.shared().approvals.decide(&p.approval_id, false, p.feedback)?;
                Ok(json!({ "approval": approval }))
            },
        );
}
