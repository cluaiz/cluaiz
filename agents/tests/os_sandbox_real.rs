//! The OS sandbox on a real Linux or macOS machine (plan `.agent/plans/security/os-sandbox.md`, step 4).
//! Ignored by default: it needs bubblewrap (Linux) or sandbox-exec (macOS) and must run on a test
//! machine, never the developer's own. Run:
//! `cargo test -p cluaiz-agents --test os_sandbox_real -- --ignored --nocapture`
//! Prints only allowed / denied per check, never the content of a file.

use std::path::{Path, PathBuf};
use std::process::Command;

use cluaiz_agents::execution::os_sandbox::{confine, detect, Backend, SandboxMode, SandboxPolicy};

/// Runs `sh -c script` confined by `policy`; true when it exits 0
fn confined_ok(policy: &SandboxPolicy, script: &str) -> bool {
    let backend = detect();
    assert_ne!(backend, Backend::None, "no OS sandbox backend on this machine");
    let c = confine("sh", &["-c".to_string(), script.to_string()], policy, SandboxMode::Required, &backend).expect("confine");
    let out = Command::new(&c.program).args(&c.args).current_dir(&policy.workspace).output().expect("spawn");
    if !out.status.success() {
        // The sandbox's own error (bwrap) or the command's first error line; never file contents
        let err = String::from_utf8_lossy(&out.stderr);
        println!("    stderr: {}", err.lines().next().unwrap_or("").chars().take(160).collect::<String>());
    }
    out.status.success()
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME"))
}

fn workspace() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cluaiz-sbx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn report(name: &str, ok: bool) -> bool {
    println!("{name}: {}", if ok { "PASS" } else { "FAIL" });
    ok
}

#[test]
#[ignore = "needs bubblewrap or sandbox-exec on a test machine"]
fn real_confinement() {
    let ws = workspace();
    let agent = SandboxPolicy::for_agent(&ws);
    let h = home();
    let q = |p: &Path| format!("'{}'", p.display());
    let mut all = true;

    all &= report("workspace is writable", confined_ok(&agent, "echo hi > inside.txt && cat inside.txt >/dev/null"));
    // `$HOME` is an empty tmpfs inside the sandbox: a write there may succeed but must never reach the host
    let _ = confined_ok(&agent, &format!("touch {}", q(&h.join("cluaiz-sbx-probe"))));
    all &= report("a write to home never reaches the host", !h.join("cluaiz-sbx-probe").exists());
    if h.join(".ssh").exists() {
        all &= report("~/.ssh is hidden", !confined_ok(&agent, &format!("ls -A {} | grep -q .", q(&h.join(".ssh")))));
    }
    let cred = h.join(".cargo/credentials.toml");
    if cred.is_file() {
        all &= report("cargo credentials are hidden", !confined_ok(&agent, &format!("test -s {}", q(&cred))));
    }
    for tool in ["cargo", "node", "python3"] {
        let on_host = Command::new("sh").args(["-c", &format!("command -v {tool}")]).output().map(|o| o.status.success()).unwrap_or(false);
        if on_host {
            all &= report(&format!("{tool} runs confined"), confined_ok(&agent, &format!("{tool} --version >/dev/null")));
        }
    }
    let mut offline = agent.clone();
    offline.allow_network = false;
    all &= report("network on when allowed", confined_ok(&agent, "curl -s -o /dev/null --max-time 10 https://1.1.1.1"));
    all &= report("network off when not allowed", !confined_ok(&offline, "curl -s -o /dev/null --max-time 10 https://1.1.1.1"));

    let _ = std::fs::remove_dir_all(&ws);
    assert!(all, "a confinement check failed (see the lines above)");
}

/// A confined command must not plant code that git runs later outside the sandbox: hooks, and config
/// keys such as `core.hooksPath` or `core.fsmonitor`, or a whole new `.git` in place of the old one
#[test]
#[ignore = "needs bubblewrap or sandbox-exec and git on a test machine"]
fn git_hooks_are_read_only() {
    let ws = workspace().join("repo");
    std::fs::create_dir_all(&ws).unwrap();
    assert!(Command::new("git").arg("init").arg("-q").current_dir(&ws).status().expect("git").success(), "git init");
    // The policy reads the repository when it is made, as the Engine makes it per command
    let agent = SandboxPolicy::for_agent(&ws);
    let mut all = true;

    all &= report("git hooks are read-only", !confined_ok(&agent, "printf '#!/bin/sh\nexit 0\n' > .git/hooks/pre-commit"));
    all &= report("git config is read-only", !confined_ok(&agent, "printf '[core]\n\thooksPath = /tmp\n' >> .git/config"));
    all &= report("the .git folder cannot be swapped", !confined_ok(&agent, "mv .git .git-old"));
    all &= report("the rest of the project stays writable", confined_ok(&agent, "echo hi > file.txt && git add file.txt"));
    all &= report("no hook was planted on the host", !ws.join(".git/hooks/pre-commit").exists());

    let _ = std::fs::remove_dir_all(ws.parent().unwrap());
    assert!(all, "a git check failed (see the lines above)");
}
