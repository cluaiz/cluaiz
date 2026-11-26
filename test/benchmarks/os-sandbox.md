# OS sandbox on Windows, Linux and macOS

## Answer

| Platform | Latest result | Run |
|---|---|---|
| GitHub macOS and Ubuntu runners | 9 / 9 on both; first macOS sandbox run | 41 |
| Linux (bubblewrap), real machine | stock Ubuntu 24.04 blocks bwrap (falls back); with the AppArmor profile all pass | 28 |
| Windows (AppContainer) | home, `.ssh`, `.cluaiz`, Documents denied; workspace write and read work | 27 |

Below: every current run of this test type, newest first, in full.

<a id="run-41"></a>

## Run 41 (2026-11-22)

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

#### First run (`c9906d04`)

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

#### Second run (`21138f62`)

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

#### Not tested

- On macOS no tool lived under `$HOME` (Homebrew is in `/opt/homebrew`), so the read-root path for
  per-user toolchains ran only on Ubuntu.
- Windows: the AppContainer backend needs the `cluaiz` binary as its helper (a llama.cpp build); it was
  tested by hand in run 27.
- The full path through the agent and MCP servers in `required` mode (unit tests only).

Rerun: Actions, "OS sandbox", Run workflow; it also runs on every push that changes
`agents/src/execution/`.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-22-run41-os-sandbox-github-ci/`](../cloud/runs/2026-11-22-run41-os-sandbox-github-ci/)

<a id="run-28"></a>

## Run 28 (2026-11-21)

Machine: a fresh Google Cloud e2-standard-2 (2 vCPU, 8 GB, CPU only), Ubuntu 24.04.5, kernel 7.0.0-1017-gcp,
bubblewrap from apt, rustup in `~/.cargo` (a per-user toolchain), a dummy `~/.ssh` file and a dummy
`~/.cargo/credentials.toml`. Source: the Engine at the local commit after `e1c8881e` (copied with
`git archive`). Test: `cargo test --release -p cluaiz-agents --test os_sandbox_real -- --ignored --nocapture`
(`agents/tests/os_sandbox_real.rs`), which prints only PASS / FAIL per check. Setup script: `vm-setup.sh`.
Raw output: `raw.txt`. The VM was deleted after the run.

#### What happened, in order

1. **Stock Ubuntu 24.04: bubblewrap cannot run.** `kernel.apparmor_restrict_unprivileged_userns = 1`; a
   bwrap call fails with `setting up uid map: Permission denied`. The probe (`bwrap_works`) caught it:
   `detect()` returned no backend, so auto mode would run commands unconfined with the warning instead of
   failing every command. The test stopped at "no OS sandbox backend on this machine".
2. With an AppArmor profile that lets `/usr/bin/bwrap` create user namespaces (below), the probe passed.
3. **Bug found:** every confined command failed with `bwrap: Can't mkdir ~/.cargo/credentials: Read-only
   file system`: a secret path that does not exist was masked inside a read-only root. Fixed: only
   existing secret paths are masked (`secrets()` in `os_sandbox.rs`).
4. **Test bug found:** "home is not writable" failed because `$HOME` inside the sandbox is an empty tmpfs
   (writes succeed there and vanish). The check now asserts what matters: the write never reaches the host.
5. **Unit test bug found:** an AppContainer test used `C:\` literal paths, which Linux does not split;
   now built with `join`. On Linux: 19 / 19 os_sandbox unit tests pass.

#### Final result (after the fixes)

| Check | Result |
|---|---|
| workspace is writable | PASS |
| a write to home never reaches the host | PASS |
| `~/.ssh` is hidden | PASS |
| cargo credentials are hidden | PASS |
| `cargo --version` runs confined (rustup in `~/.cargo`) | PASS |
| `python3 --version` runs confined | PASS |
| network on when allowed (`curl https://1.1.1.1`) | PASS |
| network off when not allowed | PASS |

#### The AppArmor profile used (Ubuntu 23.10 and later)

```
abi <abi/4.0>,
include <tunables/global>
profile bwrap /usr/bin/bwrap flags=(unconfined) {
  userns,
  include if exists <local/bwrap>
}
```

Saved as `/etc/apparmor.d/bwrap`, loaded with `sudo apparmor_parser -r /etc/apparmor.d/bwrap`.

#### Not tested

- `node` (not installed on the VM), macOS (no Mac), `required` mode through the agent itself.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-21-run28-linux-os-sandbox/`](../cloud/runs/2026-11-21-run28-linux-os-sandbox/)

<a id="run-27"></a>

## Run 27 (2026-11-20)

Machine: the developer's Windows 11 laptop (build 26300). Binary: `cargo build -p cmd --release` with the
AppContainer backend (`agents/src/execution/os_sandbox/appcontainer.rs`). The helper was called directly
(`cluaiz.exe --cluaiz-os-sandbox-exec --net|--no-net --workspace <scratch folder> -- <command>`), with a
new empty scratch folder as the workspace. The checks print only READABLE / DENIED, never file contents.
Rerun: `bash sbx-test.sh` (set the paths at the top).

#### Results

| Check | Result |
|---|---|
| write and read a file in the workspace | works (`hi`, exit 0) |
| write a file in the home folder | `Access is denied`; no file created |
| list `~/.ssh`, `~/.cluaiz`, `~/Documents` | DENIED, all three |
| exit code passes through (`cmd /c exit 7`) | exit 7 |
| network off: `curl https://1.1.1.1`, `https://example.com` | fail (curl exit 7 and 6) |
| network on, first build | also failed: the `internetClient` capability was passed with attribute 0 (not enabled) |
| network on, after setting `SE_GROUP_ENABLED` | `1.1.1.1` 301, `example.com` 200 |
| PowerShell `Invoke-WebRequest` with network on | fails reading the console for its progress bar (access denied); 200 with `$ProgressPreference='SilentlyContinue'` |
| `git --version`, `cargo --version` (Program Files) | work |
| `node`, `npm` (Program Files\nodejs) | not found: the Node.js installer's folder ACL does not grant ALL APPLICATION PACKAGES |
| `python` (installed under AppData) | not found: the user profile is not readable in a container |

Files: `result-first-build.txt` (first build), `result-after-network-fix.txt` (second build), `sbx-test.sh`.

#### Decisions taken from this run

- The capability is passed enabled (`SE_GROUP_ENABLED`); without it a container has no network.
- Auto mode does not confine on Windows: Node.js and a per-user Python cannot start in a container, so
  confining by default would break common agent commands. `CLUAIZ_OS_SANDBOX=required` confines.

#### Not tested

- Through the agent itself (`terminal_runner`, PTY) and an MCP server under `required`; the wrapping is
  covered by unit tests only.
- A workspace with a large tree (`target/`, `node_modules/`): the time of the first ACL grant.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-20-run27-windows-os-sandbox/`](../cloud/runs/2026-11-20-run27-windows-os-sandbox/)

## All logs

| Run | Folder |
|---|---|
| 41 | [`2026-11-22-run41-os-sandbox-github-ci`](../cloud/runs/2026-11-22-run41-os-sandbox-github-ci/) |
| 28 | [`2026-11-21-run28-linux-os-sandbox`](../cloud/runs/2026-11-21-run28-linux-os-sandbox/) |
| 27 | [`2026-11-20-run27-windows-os-sandbox`](../cloud/runs/2026-11-20-run27-windows-os-sandbox/) |
