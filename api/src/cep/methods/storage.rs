//! `storage.*`: temporary media status and cleanup.

use crate::cep::{roots, CallCtx};
use cluaiz_cep::{Call, CepError, Meta, NoParams, Registry, Scope};
use serde_json::{json, Value};
use std::{fs, path::Path};

#[derive(Default)]
struct Usage {
    files: u64,
    bytes: u64,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add("storage.status",
        Meta::new(Scope::Read, "Temporary media storage usage")
            .error(-32005, "storage_path_invalid", "The temporary media path exists but is not a folder")
            .error(-32002, "forbidden", "The temporary media folder is a symbolic link (`data.needs: \"roots\"`)")
            .example(json!({}), json!({"file_count":0,"total_size_bytes":0,"total_size_mb":"0.00 MB"})),
        |_: CallCtx, _: Call, _: NoParams| async move {
            let path = cluaiz_core::EnvironmentManager::current().local_dir.join("temp_media");
            let usage = scan(&path)?;
            Ok(json!({"file_count":usage.files,"total_size_bytes":usage.bytes,
                      "total_size_mb":format!("{:.2} MB",usage.bytes as f64 / cluaiz_core::MIB as f64)}))
        });
    registry.add(
        "storage.clean",
        Meta::new(Scope::Write, "Removes cached temporary media")
            .destructive()
            .error(-32005, "storage_path_invalid", "The temporary media path exists but is not a folder")
            .error(-32002, "forbidden", "The temporary media folder is a symbolic link (`data.needs: \"roots\"`)")
            .error(-32010, "filesystem_error", "A file could not be removed (for example, another program holds it open)")
            .example(json!({}), json!({"removed_files":0,"freed_bytes":0})),
        |_: CallCtx, _: Call, _: NoParams| async move {
            let path = cluaiz_core::EnvironmentManager::current()
                .local_dir
                .join("temp_media");
            clean(&path)
        },
    );
}

fn scan(path: &Path) -> Result<Usage, CepError> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Usage::default()),
        Err(e) => return Err(roots::io_error("inspect temporary media directory", e)),
        Ok(meta) if meta.file_type().is_symlink() => return Err(CepError::forbidden("roots")),
        Ok(meta) if !meta.is_dir() => {
            return Err(CepError::conflict(
                "storage_path_invalid",
                "Temporary media path is not a directory",
            ))
        }
        _ => {}
    }
    let mut usage = Usage::default();
    for entry in fs::read_dir(path).map_err(|e| roots::io_error("list temporary media", e))? {
        let entry = entry.map_err(|e| roots::io_error("read temporary media entry", e))?;
        let meta = entry
            .metadata()
            .map_err(|e| roots::io_error("inspect temporary media entry", e))?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let child = scan(&entry.path())?;
            usage.files += child.files;
            usage.bytes += child.bytes;
        } else if meta.is_file() {
            usage.files += 1;
            usage.bytes += meta.len();
        }
    }
    Ok(usage)
}

fn clean(path: &Path) -> Result<Value, CepError> {
    let usage = scan(path)?;
    // scan rejects a symlink root; remove_dir_all does not follow child symlinks.
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| roots::io_error("clean temporary media", e))?;
        fs::create_dir_all(path)
            .map_err(|e| roots::io_error("recreate temporary media directory", e))?;
    }
    Ok(json!({"removed_files":usage.files,"freed_bytes":usage.bytes}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_counts_actual_files_and_cleanup_results() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("temp_media");
        fs::create_dir_all(cache.join("nested")).unwrap();
        fs::write(cache.join("one"), b"123").unwrap();
        fs::write(cache.join("nested/two"), b"45678").unwrap();
        let usage = scan(&cache).unwrap();
        assert_eq!((usage.files, usage.bytes), (2, 8));
        assert_eq!(
            clean(&cache).unwrap(),
            json!({"removed_files":2,"freed_bytes":8})
        );
        assert!(cache.is_dir());
        assert_eq!(scan(&cache).unwrap().files, 0);
        assert_eq!(
            clean(&cache).unwrap(),
            json!({"removed_files":0,"freed_bytes":0})
        );
    }

    #[test]
    fn test_storage_missing_is_empty_and_invalid_root_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("temp_media");
        assert_eq!(scan(&path).unwrap().files, 0);
        assert_eq!(clean(&path).unwrap()["removed_files"], 0);
        fs::write(&path, b"do not delete").unwrap();
        assert!(clean(&path).is_err());
        assert!(path.is_file());
    }

    #[cfg(windows)]
    #[test]
    fn test_storage_locked_file_reports_failure_instead_of_success() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("locked"), b"held").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(root.path().join("locked"))
            .unwrap();
        assert!(clean(root.path()).is_err());
        assert!(root.path().join("locked").exists());
        drop(held);
    }
}
