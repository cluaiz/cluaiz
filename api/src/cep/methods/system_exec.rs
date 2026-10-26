//! `system.exec`: runs a program on the host. Admin scope, never an MCP tool, the owner's approval
//! on every call, and a program with its arguments by default. A shell line (`cmd /C`, `sh -c`) and
//! running without approval are the owner's choices in permission.json (`exec.allow_shell`,
//! `exec.without_approval`, both off).

use crate::cep::roots::invalid;
use crate::cep::CallCtx;
use cluaiz_cep::{Call, CepError, Meta, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

/// The longest command (program, arguments or shell line together) accepted
const MAX_COMMAND_CHARS: usize = 8192;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Exec {
    /// The program to run (a name on PATH or a path); not passed through a shell
    program: Option<String>,
    /// Its arguments, each passed as it is
    #[serde(default)]
    args: Vec<String>,
    /// One shell line instead of `program`; only when permission.json `exec.allow_shell` is on
    command: Option<String>,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "system.exec",
        Meta::new(Scope::Admin, "Runs a program on the host and returns its output; the owner approves each call")
            .destructive()
            .no_mcp()
            .idempotent(false)
            .error(-32003, "approval_required", "Every call, until the owner approves it (`data.approval_id`); send it again with `_meta.approval`")
            .error(-32602, "invalid_params", "Both or neither of `program` / `command`, `args` with `command`, or a shell line while `exec.allow_shell` is off")
            .error(-32006, "too_large", "The command is over 8192 characters")
            .error(-32010, "process_failed", "The program could not be started (not found, not executable)")
            .example(
                json!({ "program": "git", "args": ["--version"] }),
                json!({ "exit_code": 0, "stdout": "git version 2.47.0\n", "stderr": "" }),
            ),
        |ctx: CallCtx, call: Call, p: Exec| async move {
            let policy = ctx.state.permission_schema.read().await.exec.clone();
            let mut process = match (p.program.as_deref().map(str::trim), p.command.as_deref().map(str::trim)) {
                (Some(program), None) if !program.is_empty() => {
                    check_length(program.len() + p.args.iter().map(String::len).sum::<usize>())?;
                    let mut process = tokio::process::Command::new(program);
                    process.args(&p.args);
                    process
                }
                (None, Some(line)) if !line.is_empty() && p.args.is_empty() => {
                    if !policy.allow_shell {
                        return Err(invalid("command", "A shell line needs permission.json `exec.allow_shell`; send `program` and `args`"));
                    }
                    check_length(line.len())?;
                    shell(line)
                }
                _ => return Err(invalid("program", "Send `program` (with optional `args`), or `command` alone")),
            };
            if !policy.without_approval {
                let target = p.program.as_deref().map(|program| [program].into_iter().chain(p.args.iter().map(String::as_str)).collect::<Vec<_>>().join(" "));
                call.require_approval(&target.or(p.command).unwrap_or_default())?;
            }
            let output = process
                .output()
                .await
                .map_err(|e| CepError::unavailable("process_failed", format!("Could not start the program: {e}")))?;
            Ok(json!({
                "exit_code": output.status.code(),
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
            }))
        },
    );
}

fn shell(line: &str) -> tokio::process::Command {
    #[cfg(target_os = "windows")]
    let (shell, flag) = ("cmd", "/C");
    #[cfg(not(target_os = "windows"))]
    let (shell, flag) = ("sh", "-c");
    let mut process = tokio::process::Command::new(shell);
    process.args([flag, line]);
    process
}

fn check_length(chars: usize) -> Result<(), CepError> {
    if chars > MAX_COMMAND_CHARS {
        return Err(CepError::too_large(MAX_COMMAND_CHARS as u64, chars as u64));
    }
    Ok(())
}
