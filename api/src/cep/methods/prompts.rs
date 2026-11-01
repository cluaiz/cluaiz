//! `prompts.*`: the instructions the Engine itself sends to a model (the terminal agent's system
//! prompt, `/init`, `/compact`, the inline tool block, notes headings, the card request). Each has an
//! id and a default; a client reads them and replaces or resets one. See `cluaiz_core::prompts`.

use crate::cep::CallCtx;
use cluaiz_cep::{Call, CepError, Meta, NoParams, Registry, Scope};
use cluaiz_core::prompts::{Store, MAX_CHARS};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Id {
    /// The prompt's id (from `prompts.list`), e.g. `agent.system`
    id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Set {
    /// The prompt's id (from `prompts.list`)
    id: String,
    /// The new text, 1 to 20000 characters. `{{name}}` is filled in for the prompt's `variables`
    text: String,
}

fn unknown(id: &str) -> CepError {
    CepError::not_found("prompt_not_found", format!("No prompt {id}; prompts.list names them"))
}

fn saved(result: Result<cluaiz_core::prompts::Prompt, String>) -> Result<Value, CepError> {
    result
        .map(|prompt| json!({ "prompt": prompt }))
        .map_err(|error| CepError::unavailable("prompt_not_saved", format!("The prompt could not be saved: {error}")))
}

pub fn register(registry: &mut Registry<CallCtx>) {
    let prompt = json!({
        "id": "tools.workspace",
        "description": "Tells a chat model with tools which folder they run in.",
        "variables": ["folder"],
        "text": "Tools run in the project folder {{folder}}. Use paths relative to it (\".\" is the project folder).",
        "overridden": false
    });
    registry
        .add(
            "prompts.list",
            Meta::new(Scope::Read, "Every prompt the Engine sends to a model: id, description, variables, the text in use, and whether it is the user's text")
                .example(json!({}), json!({ "prompts": [prompt.clone()] })),
            |_: CallCtx, _: Call, _: NoParams| async move { Ok(json!({ "prompts": Store::current().list() })) },
        )
        .add(
            "prompts.get",
            Meta::new(Scope::Read, "One prompt and the text in use")
                .error(-32004, "prompt_not_found", "No prompt with this id")
                .example(json!({ "id": "tools.workspace" }), json!({ "prompt": prompt.clone() })),
            |_: CallCtx, _: Call, p: Id| async move {
                Store::current().get(&p.id).map(|prompt| json!({ "prompt": prompt })).ok_or_else(|| unknown(&p.id))
            },
        )
        .add(
            "prompts.set",
            Meta::new(Scope::Write, "Replaces a prompt's text for every later request; the text it replaces is kept in its history")
                .idempotent(false)
                .error(-32602, "invalid_params", "`text` is empty or over 20000 characters")
                .error(-32004, "prompt_not_found", "No prompt with this id")
                .error(-32010, "prompt_not_saved", "The prompt file could not be written")
                .example(
                    json!({ "id": "tools.workspace", "text": "Work only inside {{folder}}." }),
                    json!({ "prompt": { "id": "tools.workspace", "description": "Tells a chat model with tools which folder they run in.", "variables": ["folder"], "text": "Work only inside {{folder}}.", "overridden": true } }),
                ),
            |_: CallCtx, _: Call, p: Set| async move {
                let store = Store::current();
                store.get(&p.id).ok_or_else(|| unknown(&p.id))?;
                let length = p.text.trim().chars().count();
                if length == 0 || length > MAX_CHARS {
                    return Err(CepError::invalid_params(
                        "`text` must be 1 to 20000 characters; prompts.reset goes back to the default",
                        vec![json!({ "path": "text", "message": "length" })],
                        None,
                    ));
                }
                saved(store.set(&p.id, &p.text))
            },
        )
        .add(
            "prompts.reset",
            Meta::new(Scope::Write, "Goes back to the default text; the user's text is kept in the history")
                .error(-32004, "prompt_not_found", "No prompt with this id")
                .error(-32010, "prompt_not_saved", "The prompt file could not be removed")
                .example(json!({ "id": "tools.workspace" }), json!({ "prompt": prompt.clone() })),
            |_: CallCtx, _: Call, p: Id| async move {
                let store = Store::current();
                store.get(&p.id).ok_or_else(|| unknown(&p.id))?;
                saved(store.reset(&p.id))
            },
        )
        .add(
            "prompts.history",
            Meta::new(Scope::Read, "The texts a prompt had before, newest first (the last 20)")
                .error(-32004, "prompt_not_found", "No prompt with this id")
                .example(
                    json!({ "id": "tools.workspace" }),
                    json!({ "id": "tools.workspace", "history": [{ "replaced_at": "20261101T101500.120Z", "text": "Work only inside {{folder}}." }] }),
                ),
            |_: CallCtx, _: Call, p: Id| async move {
                let history = Store::current().history(&p.id).map_err(|_| unknown(&p.id))?;
                Ok(json!({ "id": p.id, "history": history }))
            },
        );
}
