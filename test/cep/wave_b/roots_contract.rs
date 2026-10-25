//! Real filesystem tests of the production resolver; no model, server or personal home.
#[path = "../../../api/src/cep/roots.rs"]
mod roots;

use cluaiz_cep::{error, OnDenied, Principal, Scope};
use std::{fs, path::Path};

fn reader(root: &Path) -> Principal {
    Principal {
        id: "token:roots-contract".into(),
        scopes: vec![Scope::Read],
        roots: Some(vec![fs::canonicalize(root).unwrap()]),
        on_denied: OnDenied::Error,
        owner: false,
    }
}

#[test]
fn test_relative_unicode_file_resolves_against_workspace() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("नाम.md"), "content").unwrap();
    let resolved = roots::resolve("नाम.md", dir.path(), false).unwrap();
    assert_eq!(
        resolved,
        fs::canonicalize(dir.path().join("नाम.md")).unwrap()
    );
    assert!(reader(dir.path()).within_roots(&resolved));
}

#[test]
fn test_nonexistent_write_normalizes_parent_segments_before_root_check() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let resolved = roots::resolve("new/../../outside.txt", &workspace, true).unwrap();
    assert_eq!(
        resolved,
        fs::canonicalize(dir.path()).unwrap().join("outside.txt")
    );
    assert!(!reader(&workspace).within_roots(&resolved));
    assert!(!workspace.join("new").exists());
}

#[test]
fn test_missing_read_is_not_found_and_does_not_create_parent() {
    let dir = tempfile::tempdir().unwrap();
    let error = roots::resolve("missing/file.txt", dir.path(), false).unwrap_err();
    assert_eq!(error.code, error::NOT_FOUND);
    assert_eq!(error.kind(), Some("file_not_found"));
    assert!(!dir.path().join("missing").exists());
}

#[test]
fn test_write_under_missing_parents_is_resolved_without_creating_them() {
    let dir = tempfile::tempdir().unwrap();
    let resolved = roots::resolve("new/sub/file.txt", dir.path(), true).unwrap();
    assert_eq!(
        resolved,
        fs::canonicalize(dir.path())
            .unwrap()
            .join("new/sub/file.txt")
    );
    assert!(reader(dir.path()).within_roots(&resolved));
    assert!(!resolved.exists());
}

#[test]
fn test_invalid_empty_or_nul_path_is_invalid_params() {
    let dir = tempfile::tempdir().unwrap();
    for input in ["", "file\0name"] {
        assert_eq!(
            roots::resolve(input, dir.path(), true).unwrap_err().code,
            error::INVALID_PARAMS
        );
    }
}

#[test]
fn test_display_preserves_unc_server_prefix() {
    assert_eq!(
        roots::display(Path::new(r"\\?\UNC\server\share\file")),
        r"\\server\share\file"
    );
    assert_eq!(
        roots::display(Path::new(r"\\?\C:\project\file")),
        r"C:\project\file"
    );
}

#[test]
fn test_existing_parent_traversal_is_outside_token_root() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    fs::write(dir.path().join("outside.txt"), "outside").unwrap();
    let resolved = roots::resolve("../outside.txt", &workspace, false).unwrap();
    assert!(!reader(&workspace).within_roots(&resolved));
}

#[cfg(windows)]
#[test]
fn test_drive_relative_and_root_relative_paths_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    for input in [r"C:relative.txt", r"\relative.txt"] {
        assert_eq!(
            roots::resolve(input, dir.path(), true).unwrap_err().code,
            error::INVALID_PARAMS
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn test_symlink_to_outside_is_canonicalized_outside_root() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    let outside = dir.path().join("outside");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "secret").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, workspace.join("link")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&outside, workspace.join("link")).unwrap();
    let principal = reader(&workspace);
    for (path, write) in [
        ("link/secret.txt", false),
        ("link/new.txt", true),
        ("missing/../link/new.txt", true),
    ] {
        let resolved = roots::resolve(path, &workspace, write).unwrap();
        assert!(
            !principal.within_roots(&resolved),
            "{path} escaped token roots"
        );
    }
}
