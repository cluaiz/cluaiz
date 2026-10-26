//! `workspace.*`: the Engine's workspace folder.

use crate::cep::{roots, CallCtx};
use cluaiz_cep::{Call, Meta, NoParams, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetParams {
    /// An existing folder: absolute, or relative to the current workspace
    path: String,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "workspace.get",
        Meta::new(Scope::Read, "Returns the current workspace directory")
            .example(json!({}), json!({"path": "/project"})),
        |ctx: CallCtx, call: Call, _: NoParams| async move {
            let workspace = ctx.state.current_workspace_dir.read().await.clone();
            let path = roots::resolve(".", &workspace, false)?;
            call.check_path(&path)?;
            Ok(json!({"path": roots::display(&path)}))
        },
    );
    registry.add(
        "workspace.set",
        Meta::new(
            Scope::Write,
            "Changes the workspace to an existing directory",
        )
        .no_mcp()
        .error(-32602, "invalid_params", "The path is not a folder, or is drive-relative / root-relative")
        .error(-32004, "file_not_found", "The folder does not exist")
        .error(-32002, "forbidden", "The folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .example(json!({"path": "/project"}), json!({"path": "/project"})),
        |ctx: CallCtx, call: Call, params: SetParams| async move {
            let workspace = ctx.state.current_workspace_dir.read().await.clone();
            let path = roots::resolve(&params.path, &workspace, false)?;
            call.check_path(&path)?;
            if !path.is_dir() {
                return Err(roots::invalid("path", "Workspace must be a directory"));
            }
            assign(&ctx, &path).await;
            Ok(json!({"path": roots::display(&path)}))
        },
    );
}

pub(super) async fn assign(ctx: &CallCtx, path: &std::path::Path) {
    // Keep the existing App/agent workspace owner; do not create a second workspace store.
    let mut workspace = ctx.state.current_workspace_dir.write().await;
    *workspace = path.to_path_buf();
    cluaiz_agents::ToolsEngine::set_workspace_dir(path.to_path_buf());
}
