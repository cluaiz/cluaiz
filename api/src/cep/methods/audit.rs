//! `system.audit`: the newest lines of the CEP audit log: write, exec and admin calls and every
//! denial, with the params only as a hash.

use crate::cep::CallCtx;
use cluaiz_cep::{Call, Meta, Registry, Scope};
use cluaiz_security::audit::AuditFile;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

/// The most lines one call returns
const MAX_LINES: usize = 1000;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Recent {
    /// How many of the newest lines (default 100, at most 1000)
    #[serde(default)]
    limit: Option<usize>,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "system.audit",
        Meta::new(Scope::Admin, "The newest lines of the CEP audit log, newest first").no_mcp().example(
            json!({ "limit": 1 }),
            json!({ "path": "C:/Users/me/.cluaiz/cep-audit.jsonl", "lines": [{ "ts": "2026-10-25T10:30:00.000Z", "principal": "owner",
                    "method": "fs.write", "scope": "write", "params_sha256": "9f2c…", "code": null, "ms": 3 }] }),
        ),
        |_: CallCtx, _: Call, p: Recent| async move {
            let audit = AuditFile::in_home();
            let limit = p.limit.unwrap_or(100).min(MAX_LINES);
            let lines = tokio::task::spawn_blocking(move || (audit.path().clone(), audit.recent(limit)))
                .await
                .map_err(|_| cluaiz_cep::CepError::internal("system.audit"))?;
            Ok(json!({ "path": lines.0, "lines": lines.1 }))
        },
    );
}
