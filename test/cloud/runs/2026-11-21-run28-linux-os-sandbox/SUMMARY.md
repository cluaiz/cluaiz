# Run 28: Linux OS sandbox (bubblewrap) on a real machine, 2026-11-21

Machine: a fresh Google Cloud e2-standard-2 (2 vCPU, 8 GB, CPU only), Ubuntu 24.04.5, kernel 7.0.0-1017-gcp,
bubblewrap from apt, rustup in `~/.cargo` (a per-user toolchain), a dummy `~/.ssh` file and a dummy
`~/.cargo/credentials.toml`. Source: the Engine at the local commit after `e1c8881e` (copied with
`git archive`). Test: `cargo test --release -p cluaiz-agents --test os_sandbox_real -- --ignored --nocapture`
(`agents/tests/os_sandbox_real.rs`), which prints only PASS / FAIL per check. Setup script: `vm-setup.sh`.
Raw output: `raw.txt`. The VM was deleted after the run.

## What happened, in order

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

## Final result (after the fixes)

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

## The AppArmor profile used (Ubuntu 23.10 and later)

```
abi <abi/4.0>,
include <tunables/global>
profile bwrap /usr/bin/bwrap flags=(unconfined) {
  userns,
  include if exists <local/bwrap>
}
```

Saved as `/etc/apparmor.d/bwrap`, loaded with `sudo apparmor_parser -r /etc/apparmor.d/bwrap`.

## Not tested

- `node` (not installed on the VM), macOS (no Mac), `required` mode through the agent itself.
