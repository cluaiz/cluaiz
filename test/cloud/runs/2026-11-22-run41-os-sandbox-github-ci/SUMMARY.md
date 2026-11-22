# Run 41: OS sandbox on GitHub's macOS and Ubuntu runners, 2026-11-22

The real OS sandbox checks (`agents/tests/os_sandbox_real.rs`, the same test as run 28) and the
os_sandbox unit tests, run by `.github/workflows/os-sandbox.yml` on two GitHub-hosted runners. First time
the macOS backend (sandbox-exec) ran on a real Mac. Both runs are public:

| Run | Commit | Link |
|---|---|---|
| first | `c9906d04` | https://github.com/cluaiz/cluaiz/actions/runs/44973794679 |
| second, after the fix | `21138f62` | https://github.com/cluaiz/cluaiz/actions/runs/44975252052 |

Files: `log-<run>.txt` (the full job logs without the `Compiling` / `Downloaded` lines; GitHub masks the
job token as `***`), `run-<run>.json` (commit, jobs, times).

| | macOS | Ubuntu |
|---|---|---|
| Runner image | `macos-15-arm64`, macOS 15.8.2 (24H232) | `ubuntu-24.04` |
| Sandbox | `/usr/bin/sandbox-exec` | bubblewrap 0.9.0 from apt, the AppArmor profile from run 28 (`kernel.apparmor_restrict_unprivileged_userns = 1` on the runner) |
| Tools on the host | cargo, node, python3 in `/opt/homebrew` | cargo in `~/.cargo/bin`, node in `/usr/local/bin`, python3 in `/usr/bin` |
| Dummy secrets | `~/.ssh/ci_dummy`, `~/.cargo/credentials.toml` | same |

## First run (`c9906d04`)

- **macOS: 9 / 9 real checks and 19 / 19 unit tests pass.**
- **Ubuntu: every confined command failed** with `bwrap: Can't find source path /home/runner/.local/bin:
  No such file or directory`. The sandbox binds each toolchain folder on `PATH` under `$HOME` read-only
  (so cargo, node and python installed per user still run); the runner has `~/.local/bin` on `PATH` but
  no such folder, and bubblewrap fails the whole command when a bind source is missing. A user with a
  stale `PATH` entry would have had every agent command fail. The "hidden" checks showed PASS only
  because every command failed; the run counts as failed. Run 28 did not catch it: that VM had no
  missing folder on `PATH`. Same class as run 28's mask bug (`Can't mkdir ~/.cargo/credentials`).
- Fix (`21138f62`): `SandboxPolicy::for_agent` keeps only toolchain roots that exist
  (`agents/src/execution/os_sandbox.rs`).

## Second run (`21138f62`)

| Check | macOS | Ubuntu |
|---|---|---|
| workspace is writable | PASS | PASS |
| a write to home never reaches the host | PASS (`touch: ... Operation not permitted`) | PASS |
| `~/.ssh` is hidden | PASS (`ls: ... Operation not permitted`) | PASS (`ls: cannot access '/home/runner/.ssh': No such file or directory`) |
| cargo credentials are hidden | PASS | PASS |
| cargo runs confined | PASS | PASS (from `~/.cargo`, a read root under home) |
| node runs confined | PASS | PASS |
| python3 runs confined | PASS | PASS |
| network on when allowed | PASS | PASS |
| network off when not allowed | PASS | PASS |
| os_sandbox unit tests | 19 / 19 | 19 / 19 |

The denials in the log are the sandbox's own messages, so the "hidden" checks are real this time.

## Not tested

- On macOS no tool lived under `$HOME` (Homebrew is in `/opt/homebrew`), so the read-root path for
  per-user toolchains ran only on Ubuntu.
- Windows: the AppContainer backend needs the `cluaiz` binary as its helper (a llama.cpp build); it was
  tested by hand in run 27.
- The full path through the agent and MCP servers in `required` mode (unit tests only).

Rerun: Actions, "OS sandbox", Run workflow; it also runs on every push that changes
`agents/src/execution/`.
