//! OS-level confinement of agent commands (plan: `.agent/plans/security/os-sandbox.md`).
//!
//! The allow / ask / deny policy (`policy.rs`) decides whether a command may run; this module decides
//! what the running process can touch. Linux uses bubblewrap: the whole filesystem read-only, a fresh
//! `/proc`, `/dev` and `/tmp`, `$HOME` and the Cluaiz home hidden, the workspace bound back writable, no
//! network unless allowed. The argument order follows Jan's `jail.rs` (`competitor-repos/jan@da44e32`):
//! masks first, then extra roots, then the workspace, so nothing later can shadow the one writable path.
//! macOS and Windows backends are later steps; until then they report `Backend::None`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

/// `security.os_sandbox`: what to do with and without a backend
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SandboxMode {
    /// Confine when a backend exists; otherwise run unconfined and say so
    #[default]
    Auto,
    /// Refuse to run a command without a backend
    Required,
    /// Never confine (the behaviour before this module)
    Off,
}

/// The confinement tool found on this machine
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    Bubblewrap(PathBuf),
    None,
}

impl Backend {
    pub fn name(&self) -> &'static str {
        match self {
            Backend::Bubblewrap(_) => "bubblewrap",
            Backend::None => "none",
        }
    }
}

/// bubblewrap is trusted only at fixed system paths, never through `PATH` (a planted `bwrap` earlier in
/// `PATH` would otherwise decide the confinement)
const BWRAP_PATHS: [&str; 2] = ["/usr/bin/bwrap", "/bin/bwrap"];

/// The backend available here
pub fn detect() -> Backend {
    if cfg!(target_os = "linux") {
        for path in BWRAP_PATHS {
            if Path::new(path).is_file() {
                return Backend::Bubblewrap(PathBuf::from(path));
            }
        }
    }
    Backend::None
}

/// What the confined command may touch
#[derive(Debug, Clone, Default)]
pub struct SandboxPolicy {
    /// The one writable folder (the command's working folder)
    pub workspace: PathBuf,
    /// Hidden behind an empty tmpfs: `$HOME`, the Cluaiz home
    pub hidden: Vec<PathBuf>,
    /// Bound back read-only after the masks (for example a package cache under `$HOME`)
    pub read_roots: Vec<PathBuf>,
    /// Bound back writable after the masks
    pub write_roots: Vec<PathBuf>,
    pub allow_network: bool,
}

/// The program and arguments to spawn, and which backend confines them
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confined {
    pub program: String,
    pub args: Vec<String>,
    pub backend: &'static str,
}

/// Wraps `program args` for `backend` under `mode`. Without a backend: `Auto` and `Off` return the
/// command unchanged with backend "none", `Required` refuses.
pub fn confine(program: &str, args: &[String], policy: &SandboxPolicy, mode: SandboxMode, backend: &Backend) -> Result<Confined> {
    let plain = || Confined { program: program.to_string(), args: args.to_vec(), backend: "none" };
    if mode == SandboxMode::Off {
        return Ok(plain());
    }
    match backend {
        Backend::Bubblewrap(bwrap) => {
            let mut wrapped = bwrap_args(policy);
            wrapped.push("--".into());
            wrapped.push(program.to_string());
            wrapped.extend(args.iter().cloned());
            Ok(Confined { program: bwrap.to_string_lossy().into_owned(), args: wrapped, backend: backend.name() })
        }
        Backend::None if mode == SandboxMode::Required => {
            bail!("security.os_sandbox is required, but no OS sandbox is available here (bubblewrap not found at /usr/bin/bwrap or /bin/bwrap)")
        }
        Backend::None => Ok(plain()),
    }
}

/// bubblewrap arguments, in the order that keeps the workspace the last word on its own path
fn bwrap_args(policy: &SandboxPolicy) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    let p = |path: &Path| path.to_string_lossy().into_owned();

    push(&["--ro-bind", "/", "/"]);
    push(&["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
    for hide in &policy.hidden {
        push(&["--tmpfs", &p(hide)]);
    }
    for root in &policy.read_roots {
        if !policy.write_roots.contains(root) {
            push(&["--ro-bind", &p(root), &p(root)]);
        }
    }
    for root in &policy.write_roots {
        push(&["--bind", &p(root), &p(root)]);
    }
    let ws = p(&policy.workspace);
    push(&["--bind", &ws, &ws, "--chdir", &ws]);
    // A workspace that contains a hidden path (an agent started in `$HOME`) has just shown it again:
    // mask it once more
    for hide in &policy.hidden {
        if hide != &policy.workspace && hide.starts_with(&policy.workspace) {
            push(&["--tmpfs", &p(hide)]);
        }
    }
    if !policy.allow_network {
        push(&["--unshare-net"]);
    }
    push(&["--unshare-pid", "--die-with-parent", "--new-session"]);
    a
}

/// The mode from `CLUAIZ_OS_SANDBOX` (auto | required | off; default auto). A settings field replaces
/// this in a later step of the plan.
pub fn mode_from_env() -> SandboxMode {
    match std::env::var("CLUAIZ_OS_SANDBOX").unwrap_or_default().trim().to_ascii_lowercase().as_str() {
        "required" => SandboxMode::Required,
        "off" => SandboxMode::Off,
        _ => SandboxMode::Auto,
    }
}

/// The policy for a command run in `workspace`: `$HOME`, its secret folders and the Cluaiz home hidden.
/// Network stays allowed until the execution policy has a network permission (plan, step 3 note).
pub fn policy_for(workspace: &Path) -> SandboxPolicy {
    let mut hidden = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for sub in [".ssh", ".gnupg", ".aws", ".config/gh", ".docker"] {
            hidden.push(home.join(sub));
        }
        hidden.push(home.join(".cluaiz"));
        hidden.insert(0, home);
    }
    if let Some(cluaiz_home) = std::env::var_os("CLUAIZ_HOME").map(PathBuf::from) {
        hidden.push(cluaiz_home);
    }
    SandboxPolicy { workspace: workspace.to_path_buf(), hidden, allow_network: true, ..Default::default() }
}

/// Confines `program args` run in `workspace` with the mode and backend of this machine. Says once per
/// process when commands run unconfined in auto mode.
pub fn confine_here(program: &str, args: &[String], workspace: &Path) -> Result<Confined> {
    static BACKEND: std::sync::OnceLock<Backend> = std::sync::OnceLock::new();
    static NOTICE: std::sync::Once = std::sync::Once::new();
    let backend = BACKEND.get_or_init(detect);
    let mode = mode_from_env();
    let confined = confine(program, args, &policy_for(workspace), mode, backend)?;
    if mode == SandboxMode::Auto && confined.backend == "none" {
        NOTICE.call_once(|| tracing::warn!("Agent commands are not OS-confined here (no bubblewrap at /usr/bin/bwrap or /bin/bwrap); the allow/ask/deny policy still applies"));
    }
    Ok(confined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> SandboxPolicy {
        SandboxPolicy {
            workspace: PathBuf::from("/home/u/project"),
            hidden: vec![PathBuf::from("/home/u"), PathBuf::from("/home/u/.cluaiz")],
            read_roots: vec![PathBuf::from("/home/u/.cargo")],
            write_roots: vec![],
            allow_network: false,
        }
    }

    fn bwrap() -> Backend {
        Backend::Bubblewrap(PathBuf::from("/usr/bin/bwrap"))
    }

    fn pos(args: &[String], seq: &[&str]) -> usize {
        args.windows(seq.len())
            .position(|w| w.iter().zip(seq).all(|(a, b)| a == b))
            .unwrap_or_else(|| panic!("{seq:?} not in {args:?}"))
    }

    #[test]
    fn bubblewrap_hides_home_and_binds_the_workspace_last() {
        let c = confine("sh", &["-c".into(), "ls".into()], &policy(), SandboxMode::Auto, &bwrap()).unwrap();
        assert_eq!(c.program, "/usr/bin/bwrap");
        assert_eq!(c.backend, "bubblewrap");
        let a = &c.args;
        assert_eq!(&a[..3], ["--ro-bind", "/", "/"]);
        let home = pos(a, &["--tmpfs", "/home/u"]);
        let cargo = pos(a, &["--ro-bind", "/home/u/.cargo", "/home/u/.cargo"]);
        let ws = pos(a, &["--bind", "/home/u/project", "/home/u/project"]);
        assert!(home < cargo && cargo < ws, "masks, then read roots, then the workspace");
        // the command itself comes after `--`, unchanged
        let sep = a.iter().position(|x| x == "--").unwrap();
        assert_eq!(&a[sep + 1..], ["sh", "-c", "ls"]);
        assert!(ws < sep);
    }

    #[test]
    fn network_is_cut_unless_allowed() {
        let off = confine("curl", &[], &policy(), SandboxMode::Auto, &bwrap()).unwrap();
        assert!(off.args.contains(&"--unshare-net".to_string()));
        let mut allowed = policy();
        allowed.allow_network = true;
        let on = confine("curl", &[], &allowed, SandboxMode::Auto, &bwrap()).unwrap();
        assert!(!on.args.contains(&"--unshare-net".to_string()));
    }

    #[test]
    fn a_root_that_is_also_writable_is_bound_once_writable() {
        let mut p = policy();
        p.write_roots = vec![PathBuf::from("/home/u/.cargo")];
        let c = confine("cargo", &[], &p, SandboxMode::Auto, &bwrap()).unwrap();
        assert!(!c.args.windows(2).any(|w| w[0] == "--ro-bind" && w[1] == "/home/u/.cargo"));
        pos(&c.args, &["--bind", "/home/u/.cargo", "/home/u/.cargo"]);
    }

    #[test]
    fn without_a_backend_auto_runs_plain_and_required_refuses() {
        let args = vec!["-c".to_string(), "ls".to_string()];
        let auto = confine("sh", &args, &policy(), SandboxMode::Auto, &Backend::None).unwrap();
        assert_eq!((auto.program.as_str(), auto.args.clone(), auto.backend), ("sh", args.clone(), "none"));
        assert!(confine("sh", &args, &policy(), SandboxMode::Required, &Backend::None).is_err());
    }

    #[test]
    fn a_workspace_at_home_masks_the_secret_folders_again() {
        let mut p = policy();
        p.workspace = PathBuf::from("/home/u");
        p.hidden.push(PathBuf::from("/home/u/.ssh"));
        let c = confine("sh", &[], &p, SandboxMode::Auto, &bwrap()).unwrap();
        let ws = pos(&c.args, &["--bind", "/home/u", "/home/u"]);
        let last_ssh = c.args.windows(2).rposition(|w| w[0] == "--tmpfs" && w[1] == "/home/u/.ssh").unwrap();
        assert!(last_ssh > ws, "the secret folder is masked after the workspace bind");
    }

    #[test]
    fn off_never_wraps_even_with_a_backend() {
        let c = confine("sh", &[], &policy(), SandboxMode::Off, &bwrap()).unwrap();
        assert_eq!((c.program.as_str(), c.backend), ("sh", "none"));
    }
}
