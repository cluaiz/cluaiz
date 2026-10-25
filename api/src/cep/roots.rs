//! Shared canonical filesystem paths for CEP and attachment consumers.
//! Moved from `handlers/fs_jail.rs`; authorization belongs to `Call::check_path`.

use cluaiz_cep::CepError;
use std::path::{Component, Path, PathBuf};

/// Resolve relative paths against the workspace, including not-yet-created writes.
/// Missing components are folded after the existing ancestor is canonicalized,
/// so `new/../../outside` cannot keep a misleading workspace prefix.
pub fn resolve(input: &str, workspace: &Path, write: bool) -> Result<PathBuf, CepError> {
    if input.is_empty() || input.contains('\0') {
        return Err(invalid("path", "A nonempty filesystem path is required"));
    }
    let raw = Path::new(input);
    if !raw.is_absolute()
        && matches!(
            raw.components().next(),
            Some(Component::Prefix(_) | Component::RootDir)
        )
    {
        return Err(invalid(
            "path",
            "Drive-relative and root-relative paths are ambiguous",
        ));
    }
    let candidate = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        workspace.join(raw)
    };
    match std::fs::canonicalize(&candidate) {
        Ok(path) => return Ok(path),
        Err(error) if write && error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("resolve path", error)),
    }

    let mut ancestor = candidate.as_path();
    let mut resolved = loop {
        match std::fs::canonicalize(ancestor) {
            Ok(path) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| invalid("path", "Path has no existing ancestor"))?;
            }
            Err(error) => return Err(io_error("resolve ancestor", error)),
        }
    };
    let remainder = candidate
        .strip_prefix(ancestor)
        .map_err(|_| invalid("path", "Cannot resolve path relative to its ancestor"))?;
    for component in remainder.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir => {
                if !resolved.pop() {
                    return Err(invalid("path", "Path escapes the filesystem root"));
                }
            }
            Component::Normal(name) => resolved.push(name),
            _ => return Err(invalid("path", "Unexpected absolute path component")),
        }
        // A later component can become existing again after `missing/..`.
        match std::fs::canonicalize(&resolved) {
            Ok(path) => resolved = path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if std::fs::symlink_metadata(&resolved).is_ok() {
                    return Err(invalid(
                        "path",
                        "Cannot write through a dangling symbolic link",
                    ));
                }
            }
            Err(error) => return Err(io_error("resolve path component", error)),
        }
    }
    Ok(resolved)
}

/// Human-readable output only: permission comparisons always use canonical paths.
pub fn display(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_owned()
    }
}

pub fn invalid(field: &str, message: &str) -> CepError {
    CepError::invalid_params(
        message,
        vec![serde_json::json!({"path": field, "message": message})],
        None,
    )
}

pub fn io_error(action: &str, error: std::io::Error) -> CepError {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound => CepError::not_found("file_not_found", format!("{action}: {error}")),
        ErrorKind::AlreadyExists => CepError::conflict("file_exists", format!("{action}: {error}")),
        ErrorKind::PermissionDenied => CepError::new(
            cluaiz_cep::error::FORBIDDEN,
            format!("{action}: access denied"),
            Some(serde_json::json!({"type": "permission_error"})),
        ),
        _ => CepError::unavailable("filesystem_error", format!("{action}: {error}")),
    }
}
