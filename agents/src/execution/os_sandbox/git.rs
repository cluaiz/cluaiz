//! Git files in the workspace that make git run programs later, outside the sandbox: the hook scripts,
//! and the config that can name other hooks or helpers (`core.hooksPath`, `core.fsmonitor`). A confined
//! command may change the project, but not these, so a `git commit` the user runs afterwards runs
//! nothing the command planted.

use std::path::{Path, PathBuf};

/// What every backend keeps from a confined command's writes
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GitGuard {
    /// Read-only: hooks and config of the workspace's repository, and a `.git` file itself
    pub read_only: Vec<PathBuf>,
    /// The workspace's `.git` folder: its contents stay writable (objects, index, refs), but it may
    /// not be moved or removed, since a new `.git` in its place would bring its own hooks and config
    pub dir: Option<PathBuf>,
}

/// The guard for `workspace`: only paths that exist (bubblewrap fails the whole command on a missing
/// bind source, runs 28 and 41) and that sit inside the workspace (anything outside is not writable)
pub fn guard(workspace: &Path) -> GitGuard {
    let dot = workspace.join(".git");
    let mut out = GitGuard::default();
    let repositories = if dot.is_dir() {
        out.dir = Some(dot.clone());
        vec![dot]
    } else if dot.is_file() {
        // A worktree or submodule: the file names the real git folder, so the file must not change
        out.read_only.push(dot.clone());
        linked(&dot)
    } else {
        Vec::new()
    };
    for repository in repositories {
        for name in ["hooks", "config"] {
            if let Some(path) = inside(workspace, &repository.join(name)) {
                if !out.read_only.contains(&path) {
                    out.read_only.push(path);
                }
            }
        }
    }
    out
}

/// The git folders a `.git` file points at: its `gitdir:`, and that folder's `commondir` (a worktree
/// keeps its hooks and config in the main repository's folder)
fn linked(file: &Path) -> Vec<PathBuf> {
    let base = file.parent().unwrap_or(Path::new("."));
    let Some(gitdir) = std::fs::read_to_string(file).ok().and_then(|text| text.lines().find_map(|l| l.strip_prefix("gitdir:").map(|g| base.join(g.trim())))) else {
        return Vec::new();
    };
    let common = std::fs::read_to_string(gitdir.join("commondir")).ok().map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    std::iter::once(gitdir.clone()).chain(common.map(|c| gitdir.join(c))).collect()
}

/// `path` spelled from `workspace`, when it exists and resolves inside it (a `gitdir:` may hold `..`)
fn inside(workspace: &Path, path: &Path) -> Option<PathBuf> {
    let (root, real) = (std::fs::canonicalize(workspace).ok()?, std::fs::canonicalize(path).ok()?);
    real.strip_prefix(&root).ok().map(|rest| workspace.join(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cluaiz-git-guard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_repository_keeps_its_hooks_and_config_and_its_git_folder_in_place() {
        let ws = scratch("plain");
        std::fs::create_dir_all(ws.join(".git/hooks")).unwrap();
        std::fs::write(ws.join(".git/config"), "[core]\n").unwrap();
        let guard = guard(&ws);
        assert_eq!(guard.read_only, [ws.join(".git").join("hooks"), ws.join(".git").join("config")]);
        assert_eq!(guard.dir, Some(ws.join(".git")));
        std::fs::remove_dir_all(ws.join(".git/hooks")).unwrap();
        assert_eq!(super::guard(&ws).read_only, [ws.join(".git").join("config")], "a missing path is not listed");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn a_git_file_guards_itself_and_the_folders_it_names_inside_the_workspace() {
        let ws = scratch("linked");
        let main = ws.join("main.git");
        let worktree = main.join("worktrees").join("w");
        std::fs::create_dir_all(main.join("hooks")).unwrap();
        std::fs::write(main.join("config"), "[core]\n").unwrap();
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join("commondir"), "../..\n").unwrap();
        std::fs::write(ws.join(".git"), "gitdir: main.git/worktrees/w\n").unwrap();
        let guard = guard(&ws);
        assert_eq!(guard.dir, None);
        assert_eq!(guard.read_only, [ws.join(".git"), ws.join("main.git").join("hooks"), ws.join("main.git").join("config")]);

        let outside = scratch("outside");
        std::fs::create_dir_all(outside.join("hooks")).unwrap();
        std::fs::write(ws.join(".git"), format!("gitdir: {}\n", outside.display())).unwrap();
        assert_eq!(super::guard(&ws).read_only, [ws.join(".git")], "a folder outside the workspace is not writable anyway");
        let _ = std::fs::remove_dir_all(&ws);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn no_repository_guards_nothing() {
        let ws = scratch("none");
        assert_eq!(guard(&ws), GitGuard::default());
        let _ = std::fs::remove_dir_all(&ws);
    }
}
