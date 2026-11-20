//! OS-level confinement of agent commands (plan: `.agent/plans/security/os-sandbox.md`).
//!
//! The allow / ask / deny policy (`policy.rs`) decides whether a command may run; this module decides
//! what the running process can touch. Linux uses bubblewrap: the whole filesystem read-only, a fresh
//! `/proc`, `/dev` and `/tmp`, `$HOME` and the Cluaiz home hidden, the workspace bound back writable, no
//! network unless allowed. The argument order follows Jan's `jail.rs` (`competitor-repos/jan@da44e32`):
//! masks first, then extra roots, then the workspace, so nothing later can shadow the one writable path.
//! macOS uses `sandbox-exec` with a profile holding the same rules; Windows uses an AppContainer
//! (`os_sandbox/appcontainer.rs`), started through a helper run of this binary.

pub mod appcontainer;

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
    /// macOS `sandbox-exec` with a generated profile
    Seatbelt,
    /// Windows AppContainer, through a helper run of this binary (its path)
    AppContainer(PathBuf),
    None,
}

impl Backend {
    pub fn name(&self) -> &'static str {
        match self {
            Backend::Bubblewrap(_) => "bubblewrap",
            Backend::Seatbelt => "seatbelt",
            Backend::AppContainer(_) => "appcontainer",
            Backend::None => "none",
        }
    }
}

/// bubblewrap is trusted only at fixed system paths, never through `PATH` (a planted `bwrap` earlier in
/// `PATH` would otherwise decide the confinement)
const BWRAP_PATHS: [&str; 2] = ["/usr/bin/bwrap", "/bin/bwrap"];

/// `sandbox-exec` is trusted only at its system path, as Jan does
const SEATBELT: &str = "/usr/bin/sandbox-exec";

/// The backend available here
pub fn detect() -> Backend {
    if cfg!(target_os = "linux") {
        for path in BWRAP_PATHS {
            if Path::new(path).is_file() && bwrap_works(Path::new(path)) {
                return Backend::Bubblewrap(PathBuf::from(path));
            }
        }
    }
    if cfg!(target_os = "macos") && Path::new(SEATBELT).is_file() {
        return Backend::Seatbelt;
    }
    if cfg!(windows) && appcontainer::available() {
        if let Ok(exe) = std::env::current_exe() {
            return Backend::AppContainer(exe);
        }
    }
    Backend::None
}

/// Whether bubblewrap can create its namespaces here. The binary can be installed where it cannot
/// run: WSL1, containers without user namespaces, kernels with unprivileged user namespaces turned off
/// (sysctl or AppArmor). Found but not working would make every confined command fail, so it counts
/// as no backend: auto runs unconfined with the warning, required refuses. The probe uses the same
/// namespace flags as a real command (`bwrap_args`).
fn bwrap_works(bwrap: &Path) -> bool {
    std::process::Command::new(bwrap)
        .args(["--ro-bind", "/", "/", "--proc", "/proc", "--dev", "/dev", "--unshare-net", "--unshare-pid", "--die-with-parent", "--", "/bin/sh", "-c", "true"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The backend of this machine, probed once per process
fn backend_here() -> &'static Backend {
    static BACKEND: std::sync::OnceLock<Backend> = std::sync::OnceLock::new();
    BACKEND.get_or_init(detect)
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
    let unconfined = |reason: &str| -> Result<Confined> {
        if mode == SandboxMode::Required {
            bail!("security.os_sandbox is required, but this command cannot be OS-confined: {reason}");
        }
        Ok(plain())
    };
    match backend {
        Backend::Bubblewrap(bwrap) => {
            let mut wrapped = bwrap_args(policy);
            wrapped.push("--".into());
            wrapped.push(program.to_string());
            wrapped.extend(args.iter().cloned());
            Ok(Confined { program: bwrap.to_string_lossy().into_owned(), args: wrapped, backend: backend.name() })
        }
        Backend::Seatbelt => {
            let mut wrapped = vec!["-p".to_string(), seatbelt_profile(policy)];
            wrapped.push(program.to_string());
            wrapped.extend(args.iter().cloned());
            Ok(Confined { program: SEATBELT.to_string(), args: wrapped, backend: backend.name() })
        }
        // An AppContainer grant on the home folder (or a folder above it) would rewrite the ACL of every
        // file under it, and expose all of it: refuse such a workspace instead of confining it
        // Measured on a Windows 11 dev machine (2026-11-20): Node.js from its installer (its folder does
        // not grant app containers access) and Python under AppData do not start in a container. Auto
        // must not break them, so on Windows only `required` confines
        Backend::AppContainer(_) if mode == SandboxMode::Auto => Ok(plain()),
        Backend::AppContainer(_) if covers_home(policy) => unconfined("on Windows the workspace must be a project folder, not your home folder or a folder above it"),
        Backend::AppContainer(exe) => {
            let wrapped = appcontainer::helper_args(policy, program, args);
            Ok(Confined { program: exe.to_string_lossy().into_owned(), args: wrapped, backend: backend.name() })
        }
        Backend::None => unconfined(NO_BACKEND),
    }
}

/// A Seatbelt profile with the same rules as the bubblewrap arguments. In a profile the later rule
/// wins, so the order matches bubblewrap's: everything readable, writes only to the workspace, write
/// roots and temp, hidden paths unreadable, read roots and the workspace readable again.
fn seatbelt_profile(policy: &SandboxPolicy) -> String {
    let q = |path: &Path| format!("(subpath \"{}\")", path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\""));
    let mut writable = vec![q(&policy.workspace), "(subpath \"/private/tmp\")".into(), "(subpath \"/private/var/folders\")".into(), "(subpath \"/dev\")".into()];
    writable.extend(policy.write_roots.iter().map(|r| q(r)));
    let mut profile = String::from("(version 1)
(allow default)
(deny file-write* (subpath \"/\"))
");
    profile += &format!("(allow file-write* {})
", writable.join(" "));
    if !policy.hidden.is_empty() {
        let hidden: Vec<String> = policy.hidden.iter().map(|h| q(h)).collect();
        profile += &format!("(deny file-read* file-write* {})
", hidden.join(" "));
    }
    let back: Vec<String> = policy.read_roots.iter().chain(&policy.write_roots).map(|r| q(r)).chain([q(&policy.workspace)]).collect();
    profile += &format!("(allow file-read* {})
", back.join(" "));
    let writable_back: Vec<String> = policy.write_roots.iter().map(|r| q(r)).chain([q(&policy.workspace)]).collect();
    profile += &format!("(allow file-write* {})
", writable_back.join(" "));
    // A workspace that contains a hidden path (an agent started in `$HOME`) has just reopened it: deny
    // it once more, as the bubblewrap arguments mask it again
    let again: Vec<String> = policy.hidden.iter().filter(|h| *h != &policy.workspace && h.starts_with(&policy.workspace)).map(|h| q(h)).collect();
    if !again.is_empty() {
        profile += &format!("(deny file-read* file-write* {})
", again.join(" "));
    }
    if !policy.allow_network {
        profile += "(deny network*)
(allow network* (local unix-socket))
";
    }
    profile
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

/// Folders under `$HOME` that hold credentials; hidden from every confined process
const SECRET_DIRS: [&str; 5] = [".ssh", ".gnupg", ".aws", ".config/gh", ".docker"];

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Why there is no backend, per platform
const NO_BACKEND: &str = if cfg!(target_os = "linux") {
    "bubblewrap not found at /usr/bin/bwrap or /bin/bwrap, or it cannot create namespaces here (WSL1, a container, or unprivileged user namespaces turned off)"
} else if cfg!(target_os = "macos") {
    "/usr/bin/sandbox-exec not found"
} else if cfg!(windows) {
    "AppContainer is not available (Windows 8 or later is needed)"
} else {
    "no OS sandbox exists for this platform"
};

/// Why auto mode left commands unconfined with this backend
fn unconfined_reason(backend: &Backend) -> &'static str {
    match backend {
        Backend::None => NO_BACKEND,
        Backend::AppContainer(_) => WINDOWS_AUTO,
        _ => "the workspace is the home folder or above it",
    }
}

const WINDOWS_AUTO: &str = "on Windows auto mode does not confine, because tools such as Node.js and a per-user Python cannot start inside an AppContainer; set CLUAIZ_OS_SANDBOX=required to confine";

/// Whether a granted root is the home folder or contains it
fn covers_home(policy: &SandboxPolicy) -> bool {
    let Some(home) = home() else { return false };
    std::iter::once(&policy.workspace).chain(&policy.read_roots).chain(&policy.write_roots).any(|root| home.starts_with(root))
}

/// The Cluaiz home (models, config with provider keys, tool packages)
fn cluaiz_home() -> Option<PathBuf> {
    std::env::var_os("CLUAIZ_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".cluaiz")))
}

impl SandboxPolicy {
    /// An agent command: the whole `$HOME` hidden (its secret folders masked again if the workspace is
    /// `$HOME`), the Cluaiz home hidden, the workspace writable. Network stays allowed until the execution
    /// policy has a network permission (plan, step 3 note).
    pub fn for_agent(workspace: &Path) -> Self {
        let mut hidden: Vec<PathBuf> = home().into_iter().collect();
        hidden.extend(home().into_iter().flat_map(|h| SECRET_DIRS.map(|d| h.join(d))));
        hidden.extend(cluaiz_home());
        Self { workspace: workspace.to_path_buf(), hidden, allow_network: true, ..Default::default() }
    }

    /// A tool's own process (MCP server, native plugin): `$HOME` stays readable for language caches
    /// (npm, uv), its secret folders and the Cluaiz home are hidden, the tool's package folder is
    /// bound back read-only, the workspace is writable, network as the tool's permission says.
    pub fn for_tool(workspace: &Path, package: &Path, allow_network: bool) -> Self {
        let mut hidden: Vec<PathBuf> = home().into_iter().flat_map(|h| SECRET_DIRS.map(|d| h.join(d))).collect();
        hidden.extend(cluaiz_home());
        Self { workspace: workspace.to_path_buf(), hidden, read_roots: vec![package.to_path_buf()], allow_network, ..Default::default() }
    }
}

/// One line for users: whether agent commands are OS-confined here, and how
pub fn status_line() -> String {
    let backend = backend_here();
    match (mode_from_env(), &backend) {
        (SandboxMode::Off, _) => "OS sandbox: off (CLUAIZ_OS_SANDBOX=off); only the approval policy applies.".into(),
        (_, Backend::None) => format!("OS sandbox: not available here ({NO_BACKEND}); only the approval policy applies."),
        (SandboxMode::Auto, Backend::AppContainer(_)) => format!("OS sandbox: available (AppContainer) but off: {WINDOWS_AUTO}."),
        (_, backend) => format!("OS sandbox: on ({}); commands see the workspace, not your home folder or Cluaiz's files.", backend.name()),
    }
}

/// Whether a tool's own process may use the network: its declared `network` permission and the user's
/// choice (`permissions.rs`). A tool that declares no network permission keeps the network for now, so
/// existing packages that download at start (`npx`, `uvx`) keep working; this tightens once every
/// registry package declares it.
pub fn tool_network(config_dir: &Path, tool_type: &str, id: &str, package: &Path) -> bool {
    let manifest = std::fs::read_to_string(package.join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_default();
    let declared = crate::permissions::effective(config_dir, tool_type, id, &manifest);
    declared.iter().find(|p| p.kind == "network").map_or(true, |p| p.on)
}

/// Confines `program args` under `policy` with the mode and backend of this machine. Says once per
/// process when commands run unconfined in auto mode.
pub fn confine_here(program: &str, args: &[String], policy: &SandboxPolicy) -> Result<Confined> {
    static NOTICE: std::sync::Once = std::sync::Once::new();
    let backend = backend_here();
    let mode = mode_from_env();
    let confined = confine(program, args, policy, mode, backend)?;
    if mode == SandboxMode::Auto && confined.backend == "none" {
        NOTICE.call_once(|| tracing::warn!("Commands are not OS-confined here ({}); the allow/ask/deny policy still applies", unconfined_reason(backend)));
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
    fn seatbelt_profile_hides_home_and_keeps_the_workspace() {
        let c = confine("sh", &["-c".into(), "ls".into()], &policy(), SandboxMode::Auto, &Backend::Seatbelt).unwrap();
        assert_eq!((c.program.as_str(), c.backend), ("/usr/bin/sandbox-exec", "seatbelt"));
        assert_eq!(c.args[0], "-p");
        let profile = &c.args[1];
        let deny = profile.find("(deny file-read* file-write* (subpath \"/home/u\")").unwrap();
        let back = profile.rfind("(allow file-read* (subpath \"/home/u/.cargo\")").unwrap();
        assert!(deny < back, "read roots come back after the home is hidden");
        assert!(profile.contains("(deny network*)"));
        assert_eq!(&c.args[2..], ["sh", "-c", "ls"]);
    }

    #[test]
    fn seatbelt_reopens_a_home_workspace_but_not_its_secrets() {
        let mut p = policy();
        p.workspace = PathBuf::from("/home/u");
        p.hidden.push(PathBuf::from("/home/u/.ssh"));
        let profile = seatbelt_profile(&p);
        let reopen = profile.rfind("(allow file-write* (subpath \"/home/u\")").unwrap();
        let ssh = profile.rfind("(subpath \"/home/u/.ssh\")").unwrap();
        assert!(profile[..ssh].rfind("(deny file-read* file-write*").unwrap() > reopen);
        assert!(reopen < ssh, "the workspace is usable and .ssh is denied after it");
    }

    #[test]
    fn appcontainer_reruns_this_binary_as_the_helper() {
        let exe = Backend::AppContainer(PathBuf::from("C:/bin/cluaiz.exe"));
        assert_eq!(confine("cmd.exe", &[], &policy(), SandboxMode::Auto, &exe).unwrap().backend, "none", "auto does not confine on Windows");
        let c = confine("cmd.exe", &["/c".into(), "dir".into()], &policy(), SandboxMode::Required, &exe).unwrap();
        assert_eq!((c.program.as_str(), c.backend), ("C:/bin/cluaiz.exe", "appcontainer"));
        assert_eq!(c.args[0], appcontainer::HELPER_FLAG);
        assert_eq!(&c.args[c.args.len() - 3..], ["cmd.exe", "/c", "dir"]);
    }

    #[test]
    fn appcontainer_refuses_the_home_folder_as_workspace() {
        let Some(home) = home() else { return };
        let mut p = policy();
        p.workspace = home;
        let exe = Backend::AppContainer(PathBuf::from("cluaiz.exe"));
        assert!(confine("cmd.exe", &[], &p, SandboxMode::Required, &exe).is_err());
        assert_eq!(confine("cmd.exe", &[], &p, SandboxMode::Auto, &exe).unwrap().backend, "none");
    }

    #[test]
    fn off_never_wraps_even_with_a_backend() {
        let c = confine("sh", &[], &policy(), SandboxMode::Off, &bwrap()).unwrap();
        assert_eq!((c.program.as_str(), c.backend), ("sh", "none"));
    }
}
