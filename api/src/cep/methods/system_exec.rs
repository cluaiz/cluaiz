//! `system.exec`: runs a command in the host's shell (`cmd /C` on Windows, `sh -c` elsewhere),
//! what `/v1/system/cmd` did (03-method-catalog Part B). Admin scope and never an MCP tool; whether
//! every call also needs the owner's approval is open question O1. Owner: Claude (Wave A).

use crate::cep::CallCtx;
use cluaiz_cep::{Call, CepError, Meta, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

/// The longest command line accepted
const MAX_COMMAND_CHARS: usize = 8192;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Exec {
    /// One shell command line
    command: String,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "system.exec",
        Meta::new(Scope::Admin, "Runs a command in the host's shell and returns its output").destructive().no_mcp().idempotent(false).example(
            json!({ "command": "echo hi" }),
            json!({ "exit_code": 0, "stdout": "hi\n", "stderr": "" }),
        ),
        |_: CallCtx, _: Call, p: Exec| async move {
            let command = p.command.trim();
            if command.is_empty() {
                return Err(CepError::invalid_params("`command` cannot be empty", vec![json!({ "path": "command", "message": "empty" })], None));
            }
            if command.len() > MAX_COMMAND_CHARS {
                return Err(CepError::too_large(MAX_COMMAND_CHARS as u64, command.len() as u64));
            }
            #[cfg(target_os = "windows")]
            let output = tokio::process::Command::new("cmd").args(["/C", command]).output().await;
            #[cfg(not(target_os = "windows"))]
            let output = tokio::process::Command::new("sh").arg("-c").arg(command).output().await;
            let output = output.map_err(|e| CepError::unavailable("process_failed", format!("Could not start the shell: {e}")))?;
            Ok(json!({
                "exit_code": output.status.code(),
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
            }))
        },
    );
}
