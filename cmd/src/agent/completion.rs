//! Passive workspace filename discovery. Files are read only by the existing authorized tools.

use cluaiz_terminal::app::Suggestion;
use std::path::Path;

pub fn suggestions(root: &Path, draft: &str) -> (usize, Vec<Suggestion>) {
    if draft.starts_with('/') && !draft.contains(char::is_whitespace) {
        return (0, super::commands::suggestions(draft));
    }
    let Some(at) = draft
        .rfind('@')
        .filter(|at| *at == 0 || draft[..*at].ends_with(char::is_whitespace))
    else {
        return (0, Vec::new());
    };
    let query = draft[at + 1..].trim_matches('"');
    if query.contains(['\n', '\r']) {
        return (at, Vec::new());
    }
    let path = Path::new(query);
    let (parent, prefix) = if query.is_empty() || query.ends_with(['/', '\\']) {
        (
            if query.is_empty() {
                Path::new(".")
            } else {
                path
            },
            String::new(),
        )
    } else {
        (
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
        )
    };
    let Ok(directory) = cluaiz_api::cep::roots::resolve(&parent.to_string_lossy(), root, false)
    else {
        return (at, Vec::new());
    };
    if !directory.starts_with(root) {
        return (at, Vec::new());
    }
    let entries = super::tools::directory_entries(&directory).unwrap_or_default();
    let items = entries
        .into_iter()
        .filter(|(_, name)| {
            name.to_lowercase().starts_with(&prefix) && !name.chars().any(char::is_control)
        })
        .take(30)
        .map(|(is_dir, name)| {
            let relative = directory
                .strip_prefix(root)
                .unwrap_or(Path::new("."))
                .join(name);
            let mut value = relative.to_string_lossy().replace('\\', "/");
            if is_dir {
                value.push('/');
            }
            let value = if value.contains(char::is_whitespace) {
                format!("@\"{value}\"")
            } else {
                format!("@{value}")
            };
            Suggestion {
                value,
                description: if is_dir {
                    "Folder".into()
                } else {
                    "File reference".into()
                },
            }
        })
        .collect();
    (at, items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commands_and_spaced_unicode_files_use_workspace_boundary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note नमस्ते.md"), "content is not needed").unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert_eq!(suggestions(&root, "/th").1[0].value, "/theme");
        assert_eq!(
            suggestions(&root, "read @no").1[0].value,
            "@\"note नमस्ते.md\""
        );
        assert!(suggestions(&root, "mail@example.com").1.is_empty());
        assert!(suggestions(&root, "@../").1.is_empty());
        assert!(suggestions(&root, "@missing/").1.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn test_completion_does_not_enumerate_an_external_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("private.txt"), "private").unwrap();
        // A symlink may need Developer Mode. Do not report coverage if creation fails.
        match std::os::windows::fs::symlink_dir(outside.path(), root.path().join("link")) {
            Ok(()) => assert!(suggestions(&root.path().canonicalize().unwrap(), "@link/")
                .1
                .is_empty()),
            Err(error) => eprintln!("symlink scenario unavailable on this host: {error}"),
        }
    }
}
