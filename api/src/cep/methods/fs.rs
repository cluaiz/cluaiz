//! `fs.*`: files inside the caller's roots. Paths go through `cep::roots` and `Call::check_path`.

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

// Preserve bounded recursion and report partial listings explicitly.
const MAX_LIST_DEPTH: usize = 12;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadParams {
    /// Absolute, or relative to the workspace
    path: String,
    /// Always answer `data_base64`, even for UTF-8 text
    #[serde(default)]
    binary: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteParams {
    /// Absolute, or relative to the workspace; missing parent folders are created
    path: String,
    /// The file's content as UTF-8 text (send this or `data_base64`)
    text: Option<String>,
    /// The file's content as standard base64 (send this or `text`)
    data_base64: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListParams {
    /// The folder to list (default: `root`, else the workspace)
    path: Option<String>,
    /// The folder the answered `path` values are relative to (default: the workspace)
    root: Option<String>,
    /// Also list sub-folders, at most 12 levels deep
    #[serde(default)]
    recursive: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PathParams {
    /// Absolute, or relative to the workspace
    path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TransferParams {
    /// The existing file or folder: absolute, or relative to the workspace
    from: String,
    /// The new path: absolute, or relative to the workspace; missing parent folders are created
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

#[derive(Default)]
struct ListingStats {
    truncated: bool,
    skipped: usize,
}

impl ListingStats {
    fn skip(&mut self) {
        self.truncated = true;
        self.skipped += 1;
    }
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "fs.read",
        Meta::new(
            Scope::Read,
            "Reads a UTF-8 file, or returns base64 for binary data",
        )
        .error(-32004, "file_not_found", "The path does not exist")
        .error(-32602, "invalid_params", "An empty, drive-relative or root-relative path")
        .error(-32002, "forbidden", "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)")
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
        .error(-32602, "invalid_params", "Both or neither of `text` / `data_base64`, invalid base64, or a bad path")
        .error(-32002, "forbidden", "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)")
        .example(
            json!({"path":"notes.txt","text":"hello"}),
            json!({"path":"/project/notes.txt","bytes":5}),
        ),
        |ctx: CallCtx, call: Call, p: WriteParams| blocking(ctx, call, p, write),
    );
    registry.add(
        "fs.list",
        Meta::new(Scope::Read, "Lists directory entries, optionally recursively")
            .error(-32602, "invalid_params", "The path is not a folder, or is drive-relative / root-relative")
            .error(-32004, "file_not_found", "The folder does not exist")
            .error(-32002, "forbidden", "The folder is outside the caller's roots (`data.needs: \"roots\"`)")
            .example(json!({"path":".","recursive":false}), json!({"items":[{"path":"notes.txt","name":"notes.txt","is_folder":false,"size":5,"modified":"2026-10-25T00:00:00.000Z"}],"truncated":false,"skipped":0})),
        |ctx: CallCtx, call: Call, p: ListParams| blocking(ctx, call, p, list),
    );
    registry.add(
        "fs.delete",
        Meta::new(Scope::Write, "Deletes a file or directory; a link is removed itself, never its target")
            .destructive()
            .error(-32004, "file_not_found", "The path does not exist")
            .error(-32602, "invalid_params", "The path does not end in a file or folder name (for example `..`)")
            .error(-32002, "forbidden", "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)")
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
            "Moves a file or directory between authorized paths; a link is moved itself",
        )
        .error(-32004, "file_not_found", "`from` does not exist")
        .error(-32602, "invalid_params", "`from` does not end in a file or folder name, or a path is drive-relative / root-relative")
        .error(-32005, "file_exists", "The OS refused because `to` exists and cannot be replaced")
        .error(-32002, "forbidden", "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)")
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
        .error(-32004, "file_not_found", "`from` does not exist")
        .error(-32602, "invalid_params", "`to` is inside `from`, or the copied folder holds a link cycle")
        .error(-32002, "forbidden", "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)")
        .example(
            json!({"from":"notes.txt","to":"copy.txt"}),
            json!({"from":"/project/notes.txt","to":"/project/copy.txt"}),
        ),
        |ctx: CallCtx, call: Call, p: TransferParams| blocking(ctx, call, p, copy),
    );
    registry.add(
        "fs.mkdir",
        Meta::new(Scope::Write, "Creates a directory and missing parents")
        .error(-32602, "invalid_params", "An empty, drive-relative or root-relative path")
        .error(-32002, "forbidden", "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)")
        .example(
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
        .error(-32002, "forbidden", "The caller is not the owner (`data.needs: \"owner\"`): it opens a dialog on the host")
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
    if !dir.is_dir() {
        return Err(roots::invalid("path", "Listing path must be a directory"));
    }
    let mut stats = ListingStats::default();
    walk(
        call,
        workspace,
        &dir,
        &root,
        params.recursive,
        0,
        &mut items,
        &mut stats,
    );
    Ok(json!({"items": items,"truncated":stats.truncated,"skipped":stats.skipped}))
}

fn walk(
    call: &Call,
    workspace: &Path,
    dir: &Path,
    root: &Path,
    recursive: bool,
    depth: usize,
    items: &mut Vec<DiskItem>,
    stats: &mut ListingStats,
) {
    if depth > MAX_LIST_DEPTH {
        stats.skip();
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => {
            stats.skip();
            return;
        }
    };
    for entry in entries {
        let prepared = entry
            .map_err(|e| roots::io_error("read directory entry", e))
            .and_then(|entry| list_item(call, workspace, root, entry));
        let (item, canonical, is_dir) = match prepared {
            Ok(item) => item,
            Err(_) => {
                stats.skip();
                continue;
            }
        };
        items.push(item);
        // Do not follow directory symlinks into loops; each target was authorized above.
        if recursive && is_dir {
            walk(
                call,
                workspace,
                &canonical,
                root,
                recursive,
                depth + 1,
                items,
                stats,
            );
        }
    }
}

fn list_item(
    call: &Call,
    workspace: &Path,
    root: &Path,
    entry: fs::DirEntry,
) -> Result<(DiskItem, PathBuf, bool), CepError> {
    let entry_path = entry.path();
    let canonical = checked(call, workspace, &entry_path.to_string_lossy(), false)?;
    let kind = entry
        .file_type()
        .map_err(|e| roots::io_error("read file type", e))?;
    let metadata = entry
        .metadata()
        .map_err(|e| roots::io_error("read metadata", e))?;
    let modified = metadata
        .modified()
        .map_err(|e| roots::io_error("read modification time", e))?;
    let relative = entry_path
        .strip_prefix(root)
        .unwrap_or_else(|_| Path::new(""));
    Ok((
        DiskItem {
            path: if relative.as_os_str().is_empty() {
                entry.file_name().to_string_lossy().into()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            },
            name: entry.file_name().to_string_lossy().into(),
            is_folder: kind.is_dir(),
            size: if kind.is_dir() { 0 } else { metadata.len() },
            modified: chrono::DateTime::<chrono::Utc>::from(modified)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        },
        canonical,
        kind.is_dir(),
    ))
}

/// A link is removed or renamed itself, never its target (as `rm` / `mv` do)
fn checked_entry(call: &Call, workspace: &Path, path: &str) -> Result<PathBuf, CepError> {
    let entry = roots::resolve_entry(path, workspace)?;
    call.check_path(&entry)?;
    Ok(entry)
}

fn delete(call: &Call, workspace: &Path, params: PathParams) -> Result<Value, CepError> {
    let path = checked_entry(call, workspace, &params.path)?;
    let kind = fs::symlink_metadata(&path).map_err(|e| roots::io_error("delete path", e))?.file_type();
    let result = if kind.is_symlink() {
        // A link to a folder is a directory entry on Windows
        fs::remove_file(&path).or_else(|_| fs::remove_dir(&path))
    } else if kind.is_dir() {
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    };
    result.map_err(|e| roots::io_error("delete path", e))?;
    Ok(json!({"path": roots::display(&path)}))
}

fn rename(call: &Call, workspace: &Path, params: TransferParams) -> Result<Value, CepError> {
    let from = checked_entry(call, workspace, &params.from)?;
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
    copy_tree(workspace, &from, &to, &from, &to, &mut HashSet::new())?;
    Ok(json!({"from": roots::display(&from), "to": roots::display(&to)}))
}

fn copy_tree(
    workspace: &Path,
    from: &Path,
    to: &Path,
    source_root: &Path,
    target_root: &Path,
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
        let child_from = roots::resolve(&entry.path().to_string_lossy(), workspace, false)?;
        let child_to = roots::resolve(
            &to.join(entry.file_name()).to_string_lossy(),
            workspace,
            true,
        )?;
        if !child_from.starts_with(source_root) || !child_to.starts_with(target_root) {
            return Err(CepError::forbidden("roots"));
        }
        copy_tree(
            workspace,
            &child_from,
            &child_to,
            source_root,
            target_root,
            active,
        )?;
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
