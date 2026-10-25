//! `fs.*: files inside the caller's roots` (03-method-catalog Part B). Owner: Wave B (05-chatgpt-plan.md §2).

use crate::cep::{roots, CallCtx};
use base64::prelude::*;
use cluaiz_cep::{Call, CepError, Meta, NoParams, Registry, Scope};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

// Preserve the old listing's bounded recursion; reaching the bound is now explicit.
const MAX_LIST_DEPTH: usize = 12;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadParams {
    path: String,
    #[serde(default)]
    binary: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteParams {
    path: String,
    text: Option<String>,
    data_base64: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListParams {
    path: Option<String>,
    root: Option<String>,
    #[serde(default)]
    recursive: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PathParams {
    path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TransferParams {
    from: String,
    to: String,
}

#[derive(Serialize)]
struct DiskItem {
    path: String,
    name: String,
    is_folder: bool,
    size: u64,
    modified: String,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "fs.read",
        Meta::new(
            Scope::Read,
            "Reads a UTF-8 file, or returns base64 for binary data",
        )
        .example(
            json!({"path":"notes.txt"}),
            json!({"path":"/project/notes.txt","size":5,"binary":false,"text":"hello"}),
        ),
        |ctx: CallCtx, call: Call, p: ReadParams| blocking(ctx, call, p, read),
    );
    registry.add(
        "fs.write",
        Meta::new(
            Scope::Write,
            "Writes exactly one of text or data_base64 to a file",
        )
        .example(
            json!({"path":"notes.txt","text":"hello"}),
            json!({"path":"/project/notes.txt","bytes":5}),
        ),
        |ctx: CallCtx, call: Call, p: WriteParams| blocking(ctx, call, p, write),
    );
    registry.add(
        "fs.list",
        Meta::new(Scope::Read, "Lists directory entries, optionally recursively")
            .example(json!({"path":".","recursive":false}), json!({"items":[{"path":"notes.txt","name":"notes.txt","is_folder":false,"size":5,"modified":"SystemTime { .. }"}]})),
        |ctx: CallCtx, call: Call, p: ListParams| blocking(ctx, call, p, list),
    );
    registry.add(
        "fs.delete",
        Meta::new(Scope::Write, "Deletes a file or directory")
            .destructive()
            .example(
                json!({"path":"old.txt"}),
                json!({"path":"/project/old.txt"}),
            ),
        |ctx: CallCtx, call: Call, p: PathParams| blocking(ctx, call, p, delete),
    );
    registry.add(
        "fs.rename",
        Meta::new(
            Scope::Write,
            "Moves a file or directory between authorized paths",
        )
        .example(
            json!({"from":"old.txt","to":"new.txt"}),
            json!({"from":"/project/old.txt","to":"/project/new.txt"}),
        ),
        |ctx: CallCtx, call: Call, p: TransferParams| blocking(ctx, call, p, rename),
    );
    registry.add(
        "fs.copy",
        Meta::new(
            Scope::Write,
            "Copies a file or directory between authorized paths",
        )
        .example(
            json!({"from":"notes.txt","to":"copy.txt"}),
            json!({"from":"/project/notes.txt","to":"/project/copy.txt"}),
        ),
        |ctx: CallCtx, call: Call, p: TransferParams| blocking(ctx, call, p, copy),
    );
    registry.add(
        "fs.mkdir",
        Meta::new(Scope::Write, "Creates a directory and missing parents").example(
            json!({"path":"documents"}),
            json!({"path":"/project/documents"}),
        ),
        |ctx: CallCtx, call: Call, p: PathParams| blocking(ctx, call, p, mkdir),
    );
    registry.add(
        "fs.pick_folder",
        Meta::new(
            Scope::Read,
            "Opens the host folder picker and selects the workspace",
        )
        .owner_only()
        .example(json!({}), json!({"path": "/project"})),
        |ctx: CallCtx, call: Call, _: NoParams| async move { pick_folder(ctx, call).await },
    );
}

async fn blocking<P, F>(
    ctx: CallCtx,
    call: Call,
    params: P,
    operation: F,
) -> Result<Value, CepError>
where
    P: Send + 'static,
    F: FnOnce(&Call, &Path, P) -> Result<Value, CepError> + Send + 'static,
{
    let workspace = ctx.state.current_workspace_dir.read().await.clone();
    tokio::task::spawn_blocking(move || operation(&call, &workspace, params))
        .await
        .map_err(worker_error)?
}

fn worker_error(error: tokio::task::JoinError) -> CepError {
    let trace = uuid::Uuid::new_v4().to_string();
    tracing::error!(trace_id = %trace, %error, "CEP filesystem worker failed");
    CepError::internal(&trace)
}

fn checked(call: &Call, workspace: &Path, path: &str, write: bool) -> Result<PathBuf, CepError> {
    let resolved = roots::resolve(path, workspace, write)?;
    call.check_path(&resolved)?;
    Ok(resolved)
}

fn parents(path: &Path) -> Result<(), CepError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| roots::io_error("create parent directories", e))?;
    }
    Ok(())
}

fn read(call: &Call, workspace: &Path, params: ReadParams) -> Result<Value, CepError> {
    let path = checked(call, workspace, &params.path, false)?;
    let bytes = fs::read(&path).map_err(|e| roots::io_error("read file", e))?;
    let size = bytes.len();
    if !params.binary {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            return Ok(
                json!({"path":roots::display(&path),"size":size,"binary":false,"text":text}),
            );
        }
    }
    Ok(
        json!({"path":roots::display(&path),"size":size,"binary":true,"data_base64":BASE64_STANDARD.encode(bytes)}),
    )
}

fn write(call: &Call, workspace: &Path, params: WriteParams) -> Result<Value, CepError> {
    let bytes = match (params.text, params.data_base64) {
        (Some(text), None) => text.into_bytes(),
        (None, Some(data)) => BASE64_STANDARD
            .decode(data)
            .map_err(|_| roots::invalid("data_base64", "Invalid base64 data"))?,
        _ => {
            return Err(roots::invalid(
                "text",
                "Provide exactly one of text or data_base64",
            ))
        }
    };
    let path = checked(call, workspace, &params.path, true)?;
    parents(&path)?;
    fs::write(&path, &bytes).map_err(|e| roots::io_error("write file", e))?;
    Ok(json!({"path": roots::display(&path), "bytes": bytes.len()}))
}

fn list(call: &Call, workspace: &Path, params: ListParams) -> Result<Value, CepError> {
    let input = params
        .path
        .as_deref()
        .or(params.root.as_deref())
        .unwrap_or(".");
    let dir = checked(call, workspace, input, false)?;
    let root = checked(
        call,
        workspace,
        params.root.as_deref().unwrap_or("."),
        false,
    )?;
    let mut items = Vec::new();
    walk(
        call,
        workspace,
        &dir,
        &root,
        params.recursive,
        0,
        &mut items,
    )?;
    Ok(json!({"items": items}))
}

fn walk(
    call: &Call,
    workspace: &Path,
    dir: &Path,
    root: &Path,
    recursive: bool,
    depth: usize,
    items: &mut Vec<DiskItem>,
) -> Result<(), CepError> {
    if depth > MAX_LIST_DEPTH {
        return Err(CepError::too_large(MAX_LIST_DEPTH as u64, depth as u64));
    }
    for entry in fs::read_dir(dir).map_err(|e| roots::io_error("list directory", e))? {
        let entry = entry.map_err(|e| roots::io_error("read directory entry", e))?;
        let entry_path = entry.path();
        let canonical = checked(call, workspace, &entry_path.to_string_lossy(), false)?;
        let kind = entry
            .file_type()
            .map_err(|e| roots::io_error("read file type", e))?;
        let metadata = entry
            .metadata()
            .map_err(|e| roots::io_error("read metadata", e))?;
        let relative = entry_path
            .strip_prefix(root)
            .unwrap_or_else(|_| Path::new(""));
        items.push(DiskItem {
            path: if relative.as_os_str().is_empty() {
                entry.file_name().to_string_lossy().into()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            },
            name: entry.file_name().to_string_lossy().into(),
            is_folder: kind.is_dir(),
            size: if kind.is_dir() { 0 } else { metadata.len() },
            modified: metadata
                .modified()
                .map(|time| format!("{time:?}"))
                .unwrap_or_default(),
        });
        // Do not follow directory symlinks into loops; each target was still authorized above.
        if recursive && kind.is_dir() {
            walk(
                call,
                workspace,
                &canonical,
                root,
                recursive,
                depth + 1,
                items,
            )?;
        }
    }
    Ok(())
}

fn delete(call: &Call, workspace: &Path, params: PathParams) -> Result<Value, CepError> {
    let path = checked(call, workspace, &params.path, false)?;
    let result = if path.is_dir() {
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    };
    result.map_err(|e| roots::io_error("delete path", e))?;
    Ok(json!({"path": roots::display(&path)}))
}

fn rename(call: &Call, workspace: &Path, params: TransferParams) -> Result<Value, CepError> {
    let from = checked(call, workspace, &params.from, false)?;
    let to = checked(call, workspace, &params.to, true)?;
    parents(&to)?;
    fs::rename(&from, &to).map_err(|e| roots::io_error("rename path", e))?;
    Ok(json!({"from": roots::display(&from), "to": roots::display(&to)}))
}

fn copy(call: &Call, workspace: &Path, params: TransferParams) -> Result<Value, CepError> {
    let from = checked(call, workspace, &params.from, false)?;
    let to = checked(call, workspace, &params.to, true)?;
    if to.starts_with(&from) {
        return Err(roots::invalid("to", "Cannot copy a path into itself"));
    }
    parents(&to)?;
    copy_tree(call, workspace, &from, &to, &mut HashSet::new())?;
    Ok(json!({"from": roots::display(&from), "to": roots::display(&to)}))
}

fn copy_tree(
    call: &Call,
    workspace: &Path,
    from: &Path,
    to: &Path,
    active: &mut HashSet<PathBuf>,
) -> Result<(), CepError> {
    // Endpoints were resolved and authorized by the caller. Rechecking an outside
    // source here would consume the same single-use approval twice.
    if !from.is_dir() {
        fs::copy(from, to).map_err(|e| roots::io_error("copy file", e))?;
        return Ok(());
    }
    if !active.insert(from.to_path_buf()) {
        return Err(roots::invalid(
            "from",
            "Directory copy contains a symbolic-link cycle",
        ));
    }
    fs::create_dir_all(to).map_err(|e| roots::io_error("create copied directory", e))?;
    for entry in fs::read_dir(from).map_err(|e| roots::io_error("list copied directory", e))? {
        let entry = entry.map_err(|e| roots::io_error("read copied entry", e))?;
        let child_from = checked(call, workspace, &entry.path().to_string_lossy(), false)?;
        let child_to = checked(
            call,
            workspace,
            &to.join(entry.file_name()).to_string_lossy(),
            true,
        )?;
        copy_tree(call, workspace, &child_from, &child_to, active)?;
    }
    active.remove(from);
    Ok(())
}

fn mkdir(call: &Call, workspace: &Path, params: PathParams) -> Result<Value, CepError> {
    let path = checked(call, workspace, &params.path, true)?;
    fs::create_dir_all(&path).map_err(|e| roots::io_error("create directory", e))?;
    Ok(json!({"path": roots::display(&path)}))
}

async fn pick_folder(ctx: CallCtx, call: Call) -> Result<Value, CepError> {
    // rfd owns the platform dialog implementation; keep it off the async worker thread.
    let selected = tokio::task::spawn_blocking(|| {
        let mut dialog = rfd::FileDialog::new().set_title("Open Folder");
        if let Some(home) = dirs::home_dir() {
            dialog = dialog.set_directory(home);
        }
        dialog.pick_folder()
    })
    .await
    .map_err(worker_error)?;
    let Some(path) = selected else {
        return Ok(json!({"path": null}));
    };
    let path = roots::resolve(&path.to_string_lossy(), &path, false)?;
    call.check_path(&path)?;
    super::workspace::assign(&ctx, &path).await;
    Ok(json!({"path": roots::display(&path)}))
}
