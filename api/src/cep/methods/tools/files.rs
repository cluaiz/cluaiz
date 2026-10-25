use super::common::{self, Kind, Target};
use crate::cep::{roots, CallCtx};
use cluaiz_cep::{Call, CepError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

const MAX_TOOL_ENTRIES: usize = 10_000;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub kind: Kind,
    pub name: String,
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub kind: Kind,
    pub name: String,
    pub text: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Open {
    pub kind: Kind,
    pub name: String,
    pub path: Option<String>,
}

pub async fn read(ctx: CallCtx, call: Call, params: Read) -> Result<Value, CepError> {
    common::work(ctx, call, move |call, workspace| {
        let base = common::directory(
            &call,
            &workspace,
            &Target {
                kind: params.kind,
                name: params.name,
            },
        )?;
        let path = common::file(&base, &params.path, false)?;
        let text =
            std::fs::read_to_string(&path).map_err(|e| roots::io_error("read tool file", e))?;
        Ok(json!({"path":roots::display(&path),"text":text}))
    })
    .await
}

pub async fn write(ctx: CallCtx, call: Call, params: Write) -> Result<Value, CepError> {
    common::work(ctx, call, move |call, workspace| {
        let base = common::directory(
            &call,
            &workspace,
            &Target {
                kind: params.kind,
                name: params.name,
            },
        )?;
        let path = common::file(&base, params.kind.main_file(), false)?;
        std::fs::write(&path, &params.text).map_err(|e| roots::io_error("write tool file", e))?;
        Ok(json!({"path":roots::display(&path),"bytes":params.text.len()}))
    })
    .await
}

pub async fn open(ctx: CallCtx, call: Call, params: Open) -> Result<Value, CepError> {
    common::work(ctx, call, move |call, workspace| {
        let base = common::directory(
            &call,
            &workspace,
            &Target {
                kind: params.kind,
                name: params.name,
            },
        )?;
        let path = common::file(
            &base,
            params.path.as_deref().unwrap_or(params.kind.main_file()),
            false,
        )?;
        if !path.is_file() {
            return Err(roots::invalid("path", "Choose a file to open"));
        }
        open_editor(&path)?;
        Ok(json!({"opened":true}))
    })
    .await
}

fn open_editor(path: &Path) -> Result<(), CepError> {
    // A fixed editor program and a separate canonical path argument; no shell interpolation.
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut command = std::process::Command::new("notepad.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg("-e");
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    let mut child = command
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| roots::io_error("start file editor", error))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Package-specific tree projection, moved from the old component handler.
/// Iteration avoids recursive stack growth; symlinks are reported as skipped.
pub async fn files(ctx: CallCtx, call: Call, target: Target) -> Result<Value, CepError> {
    common::work(ctx, call, move |call, workspace| {
        let base = common::directory(&call, &workspace, &target)?;
        let mut pending = vec![base.clone()];
        let mut files = Vec::new();
        let mut skipped = 0;
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).map_err(|e| roots::io_error("list tool folder", e))? {
                if files.len() == MAX_TOOL_ENTRIES {
                    return Ok(json!({"files":files,"truncated":true,"skipped":skipped,"cache":cache(&base)?}));
                }
                let entry = entry.map_err(|e| roots::io_error("read tool entry", e))?;
                let kind = entry.file_type().map_err(|e| roots::io_error("inspect tool entry", e))?;
                if kind.is_symlink() { skipped += 1; continue; }
                let path = roots::resolve(&entry.path().to_string_lossy(), &base, false)?;
                if !path.starts_with(&base) { skipped += 1; continue; }
                let relative = path.strip_prefix(&base).map_err(common::unavailable)?;
                files.push(json!({"name":entry.file_name().to_string_lossy(),
                    "path":relative.to_string_lossy().replace('\\',"/"),"is_dir":kind.is_dir()}));
                if kind.is_dir() { pending.push(path); }
            }
        }
        files.sort_by(|a,b| a["path"].as_str().cmp(&b["path"].as_str()));
        Ok(json!({"files":files,"truncated":skipped > 0,"skipped":skipped,"cache":cache(&base)?}))
    }).await
}

pub fn cache(base: &Path) -> Result<Value, CepError> {
    let cache_path = base.join(".cache");
    match std::fs::symlink_metadata(&cache_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({"entries":[],"temp_bytes":0,"all_bytes":0}))
        }
        Err(error) => return Err(roots::io_error("inspect tool cache", error)),
        Ok(_) => {}
    }
    let path = common::file(base, ".cache", false)?;
    let mut entries = Vec::new();
    let mut temp_bytes = 0;
    let mut all_bytes = 0;
    for entry in std::fs::read_dir(path).map_err(|e| roots::io_error("read tool cache", e))? {
        let entry = entry.map_err(|e| roots::io_error("read cache entry", e))?;
        let meta = entry
            .metadata()
            .map_err(|e| roots::io_error("inspect cache entry", e))?;
        if !meta.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        all_bytes += meta.len();
        if name.starts_with("temp_") || name.ends_with(".tmp") {
            temp_bytes += meta.len();
        }
        entries.push(json!({"path":name,"size":meta.len()}));
    }
    entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(json!({"entries":entries,"temp_bytes":temp_bytes,"all_bytes":all_bytes}))
}
