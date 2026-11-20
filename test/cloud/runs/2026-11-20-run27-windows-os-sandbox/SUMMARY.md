# Run 27: Windows OS sandbox (AppContainer), 2026-11-20

Machine: the developer's Windows 11 laptop (build 26300). Binary: `cargo build -p cmd --release` with the
AppContainer backend (`agents/src/execution/os_sandbox/appcontainer.rs`). The helper was called directly
(`cluaiz.exe --cluaiz-os-sandbox-exec --net|--no-net --workspace <scratch folder> -- <command>`), with a
new empty scratch folder as the workspace. The checks print only READABLE / DENIED, never file contents.
Rerun: `bash sbx-test.sh` (set the paths at the top).

## Results

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

## Decisions taken from this run

- The capability is passed enabled (`SE_GROUP_ENABLED`); without it a container has no network.
- Auto mode does not confine on Windows: Node.js and a per-user Python cannot start in a container, so
  confining by default would break common agent commands. `CLUAIZ_OS_SANDBOX=required` confines.

## Not tested

- Through the agent itself (`terminal_runner`, PTY) and an MCP server under `required`; the wrapping is
  covered by unit tests only.
- A workspace with a large tree (`target/`, `node_modules/`): the time of the first ACL grant.
