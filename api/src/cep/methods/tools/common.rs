use crate::cep::{roots, CallCtx};
use cluaiz_agents::{installer::ToolsInstaller, ToolEntry, ToolsEngine};
use cluaiz_cep::{Call, CepError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Instructions for the model (`SKILL.md`)
    Skill,
    /// A WASM or native library (`package.json`)
    Plugin,
    /// An MCP server the Engine starts (`package.json`)
    Mcp,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Plugin => "plugin",
            Self::Mcp => "mcp",
        }
    }

    pub fn root(self) -> PathBuf {
        let env = cluaiz_core::EnvironmentManager::current();
        match self {
            Self::Skill => env.skills_dir(),
            Self::Plugin => env.plugins_dir(),
            Self::Mcp => env.mcp_dir(),
        }
    }

    pub fn main_file(self) -> &'static str {
        if self == Self::Skill {
            "SKILL.md"
        } else {
            "package.json"
        }
    }
}

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// skill, plugin or mcp
    pub kind: Kind,
    /// The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens
    pub name: String,
}

pub fn validate_name(name: &str) -> Result<(), CepError> {
    let first = name
        .bytes()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if !first
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
    {
        return Err(roots::invalid("name", "Use 1–64 lowercase letters, digits, dots, underscores or hyphens; start with a letter or digit"));
    }
    Ok(())
}

pub fn unavailable(error: impl std::fmt::Display) -> CepError {
    CepError::unavailable("tool_operation_failed", error.to_string())
}

pub fn missing(target: &Target) -> CepError {
    CepError::not_found(
        "tool_not_found",
        format!(
            "{} '{}' is not installed",
            target.kind.as_str(),
            target.name
        ),
    )
}

pub fn entry(target: &Target) -> Result<ToolEntry, CepError> {
    validate_name(&target.name)?;
    ToolsEngine::get_tool(&target.name)
        .map_err(unavailable)?
        .filter(|entry| entry.category == target.kind.as_str())
        .ok_or_else(|| missing(target))
}

/// The registered/fallback component location, with one roots decision for this call.
pub fn directory(call: &Call, workspace: &Path, target: &Target) -> Result<PathBuf, CepError> {
    validate_name(&target.name)?;
    let _ = entry(target)?;
    let path = ToolsEngine::resolve_component_dir(target.kind.as_str(), &target.name)
        .ok_or_else(|| missing(target))?;
    let path = roots::resolve(&path.to_string_lossy(), workspace, false)?;
    call.check_path(&path)?;
    Ok(path)
}

/// Installer operations use its actual primary category directory, not an arbitrary registry path.
pub fn installer_directory(
    call: &Call,
    workspace: &Path,
    target: &Target,
    installing: bool,
) -> Result<PathBuf, CepError> {
    validate_name(&target.name)?;
    if !installing && !ToolsInstaller::is_installed(target.kind.as_str(), &target.name) {
        return Err(missing(target));
    }
    let path = roots::resolve(
        &target.kind.root().join(&target.name).to_string_lossy(),
        workspace,
        installing,
    )?;
    call.check_path(&path)?;
    Ok(path)
}

/// Relative asset paths stay in their package even for an owner with unrestricted roots.
pub fn file(base: &Path, relative: &str, write: bool) -> Result<PathBuf, CepError> {
    use std::path::Component;
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\0')
        || path.is_absolute()
        || path
            .components()
            .any(|p| !matches!(p, Component::Normal(_) | Component::CurDir))
    {
        return Err(roots::invalid(
            "path",
            "Use a relative file path inside the tool package",
        ));
    }
    let path = roots::resolve(&base.join(path).to_string_lossy(), base, write)?;
    if !path.starts_with(base) {
        return Err(roots::invalid("path", "Path leaves the tool package"));
    }
    Ok(path)
}

pub async fn work<T, F>(ctx: CallCtx, call: Call, work: F) -> Result<T, CepError>
where
    T: Send + 'static,
    F: FnOnce(Call, PathBuf) -> Result<T, CepError> + Send + 'static,
{
    let workspace = ctx.state.current_workspace_dir.read().await.clone();
    tokio::task::spawn_blocking(move || work(call, workspace))
        .await
        .map_err(|error| {
            let trace = uuid::Uuid::new_v4().to_string();
            tracing::error!(trace_id = %trace, %error, "CEP tool worker failed");
            CepError::internal(&trace)
        })?
}
