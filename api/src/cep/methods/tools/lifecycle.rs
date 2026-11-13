use super::{
    common::{self, Kind, Target},
    files,
};
use crate::cep::{roots, CallCtx};
use cluaiz_agents::{installer::ToolsInstaller, ToolsEngine};
use cluaiz_cep::{Call, CepError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cache {
    /// skill, plugin or mcp
    pub kind: Kind,
    /// One tool; every tool of `kind` when absent
    pub name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Execute {
    /// skill, plugin or mcp
    pub kind: Kind,
    /// The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens
    pub name: String,
    /// The function to run (a plugin export or an MCP tool); the tool's default when absent
    #[serde(rename = "fn")]
    pub function: Option<String>,
    /// The function's arguments, as JSON
    pub input: Value,
}

/// How long an install waits for the owner to answer its questions; then they count as no
const INSTALL_QUESTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

pub async fn install(ctx: CallCtx, call: Call, target: Target) -> Result<Value, CepError> {
    common::validate_name(&target.name)?;
    let workspace = ctx.state.current_workspace_dir.read().await.clone();
    common::installer_directory(&call, &workspace, &target, true)?;
    let state = ctx.state.clone();
    call.start_task(move |handle| async move {
        if handle.is_cancelled() {
            return Err(CepError::cancelled());
        }
        // What the user's mode allows is done; the rest is asked through approvals
        let plan = ToolsInstaller::plan_install(target.kind.as_str(), &target.name)
            .await
            .map_err(common::unavailable)?;
        handle.event(json!({"stage":"planned","kind":target.kind,"name":target.name,"plan":plan}));
        let approved = if plan.has_questions() {
            let approvals = &state.cep.shared().approvals;
            let plan_json = serde_json::to_value(&plan).map_err(common::unavailable)?;
            let approval = approvals.request_for(
                "installer",
                &format!("install:{}:{}", target.kind.as_str(), target.name),
                cluaiz_cep::approval::params_digest(&plan_json),
                &plan_json.to_string(),
                Some(INSTALL_QUESTION_TIMEOUT),
            );
            handle.event(json!({"stage":"approval","kind":target.kind,"name":target.name,"approval_id":approval.id,"plan":plan_json}));
            let decision = tokio::time::timeout(INSTALL_QUESTION_TIMEOUT, approvals.wait(&approval.id)).await;
            approvals.forget(&approval.id);
            match decision {
                // Yes to everything, or the selection in `feedback`: {"tools": [...], "permissions": [...]}
                Ok(Ok(decision)) if decision.approved => decision
                    .feedback
                    .as_deref()
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                    .and_then(|value| cluaiz_agents::Approved::from_json(&value))
                    .unwrap_or_else(|| cluaiz_agents::Approved::all(&plan)),
                // Rejected, or nobody answered within the time: every question is no
                _ => cluaiz_agents::Approved::default(),
            }
        } else {
            cluaiz_agents::Approved::default()
        };
        if handle.is_cancelled() {
            return Err(CepError::cancelled());
        }
        handle.event(json!({"stage":"installing","kind":target.kind,"name":target.name}));
        let report = ToolsInstaller::install_with_plan(&plan, &approved)
            .await
            .map_err(common::unavailable)?;
        if handle.is_cancelled() {
            return Err(CepError::cancelled());
        }
        let entry = common::entry(&target)?;
        handle.event(json!({"stage":"installed","kind":target.kind,"name":target.name,"report":report}));
        cluaiz_cep::events::publish("tool.installed", json!({"kind":target.kind,"name":target.name}));
        let mut result = serde_json::to_value(entry).map_err(common::unavailable)?;
        result["install_report"] = json!(report);
        Ok(result)
    })
}

pub async fn remove(ctx: CallCtx, call: Call, target: Target) -> Result<Value, CepError> {
    common::validate_name(&target.name)?;
    let workspace = ctx.state.current_workspace_dir.read().await.clone();
    common::installer_directory(&call, &workspace, &target, false)?;
    ToolsInstaller::remove_component(target.kind.as_str(), &target.name)
        .await
        .map_err(common::unavailable)?;
    cluaiz_cep::events::publish("tool.removed", json!({"kind":target.kind,"name":target.name}));
    Ok(json!({"kind":target.kind,"name":target.name}))
}

/// Authorize a single target or the common category root once, before any deletion.
fn targets(
    call: &Call,
    workspace: &std::path::Path,
    params: &Cache,
) -> Result<Vec<(String, std::path::PathBuf)>, CepError> {
    if let Some(name) = &params.name {
        return Ok(vec![(
            name.clone(),
            common::installer_directory(
                call,
                workspace,
                &Target {
                    kind: params.kind,
                    name: name.clone(),
                },
                false,
            )?,
        )]);
    }
    let root = roots::resolve(&params.kind.root().to_string_lossy(), workspace, true)?;
    call.check_path(&root)?;
    let names = ToolsInstaller::list_installed_components(params.kind.as_str())
        .map_err(common::unavailable)?;
    names
        .into_iter()
        .map(|name| {
            common::validate_name(&name)?;
            let path = roots::resolve(
                &params.kind.root().join(&name).to_string_lossy(),
                workspace,
                false,
            )?;
            if !path.starts_with(&root) {
                return Err(roots::invalid(
                    "name",
                    "Tool folder leaves the category root",
                ));
            }
            Ok((name, path))
        })
        .collect()
}

pub async fn cache(ctx: CallCtx, call: Call, params: Cache) -> Result<Value, CepError> {
    common::work(ctx, call, move |call, workspace| {
        let entries: Result<Vec<_>, CepError> = targets(&call, &workspace, &params)?
            .into_iter()
            .map(|(name, base)| {
                let mut cache = files::cache(&base)?;
                cache["kind"] = json!(params.kind);
                cache["name"] = json!(name);
                Ok(cache)
            })
            .collect();
        Ok(json!(entries?))
    })
    .await
}

pub async fn clear_cache(ctx: CallCtx, call: Call, params: Cache) -> Result<Value, CepError> {
    common::work(ctx, call, move |call, workspace| {
        // Reject inaccessible/escaping cache folders before the existing installer touches any.
        for (_, base) in targets(&call, &workspace, &params)? {
            files::cache(&base)?;
        }
        let report =
            ToolsInstaller::clear_component_cache(params.kind.as_str(), params.name, true, true)
                .map_err(common::unavailable)?;
        Ok(json!({"removed":report.files,"freed_bytes":report.bytes}))
    })
    .await
}

pub async fn execute(ctx: CallCtx, call: Call, params: Execute) -> Result<Value, CepError> {
    common::validate_name(&params.name)?;
    if params.function.as_ref().is_some_and(|name| {
        name.is_empty() || name.contains("..") || name.contains(['/', '\\', '\0'])
    }) {
        return Err(roots::invalid(
            "fn",
            "Use a function name, not a filesystem path",
        ));
    }
    let workspace = ctx.state.current_workspace_dir.read().await.clone();
    let target = Target {
        kind: params.kind,
        name: params.name,
    };
    common::directory(&call, &workspace, &target)?;
    // A plain skill is content, not the executor's fallback pretend-workflow success.
    if target.kind == Kind::Skill {
        let base = ToolsEngine::resolve_component_dir(target.kind.as_str(), &target.name)
            .ok_or_else(|| common::missing(&target))?;
        let base = roots::resolve(&base.to_string_lossy(), &workspace, false)?;
        common::file(&base, target.kind.main_file(), false)?;
    }
    let result = ToolsEngine::execute_tool_by_name(
        target.kind.as_str(),
        &target.name,
        params.function.as_deref(),
        &params.input.to_string(),
    )
    .await
    .map_err(common::unavailable)?;
    Ok(serde_json::from_str(&result).unwrap_or(Value::String(result)))
}
