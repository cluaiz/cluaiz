# The `cluaiz` program (`cmd/`)

The command line of the Engine. `cluaiz --help` lists every command; `cluaiz <command> -h` explains one.
The full references are [docs/cmd/commands_reference.md](../docs/cmd/commands_reference.md) (commands),
[docs/cmd/agent.md](../docs/cmd/agent.md) (coding agent) and [docs/cmd/chat.md](../docs/cmd/chat.md) (chat).

| Commands | What they do |
|---|---|
| `agent`, `run` | Coding agent in the current folder; chat with a model or answer one prompt |
| `pull`, `list`, `show`, `rm`, `model`, `ps` | Download, list, inspect and remove models; choose the defaults; see what the server has loaded |
| `serve`, `mcp-server`, `token`, `permission` | HTTP server, MCP over stdio, API tokens, `permission.json` |
| `provider` | Model providers in `providers.json` |
| `status`, `calibrate`, `bench` | The hardware the planner sees, a new hardware scan, speed measurements |
| `skill`, `plugin`, `mcp` | Installed tools |
| `flow` | Automation flows, through the running server |
| `ingest`, `storage` | Document ingest; temporary storage of the running server |
| `config`, `self`, `dev-sync` | `settings.json`; update or uninstall; copy development artifacts |

## Architectural Flow

```mermaid
flowchart LR
    MAIN["src/main.rs<br/>clap: Cli / Commands"]
    CLI["src/cli/<br/>commands"]
    CHAT["src/chat/<br/>cluaiz run"]
    AGENT["src/agent/<br/>cluaiz agent"]
    UI["src/ui/ · terminal crate<br/>screens, spinner, tables"]
    API["cluaiz-api<br/>serve, providers, CEP"]
    RT["cluaiz-runtime<br/>EngineHost"]
    BUILD["build.rs<br/>Windows resources"]

    MAIN --> CLI
    MAIN --> CHAT
    MAIN --> AGENT
    CHAT --> RT
    AGENT --> RT
    CHAT --> API
    CLI --> API
    AGENT --> UI
    BUILD -.->|"at build time"| MAIN
```

## Code

| Folder | Contents |
|---|---|
| [`src/agent/`](src/agent/) | The coding agent: tools, permissions, sessions, checkpoints, its terminal screen ([README](src/agent/README.md)) |
| [`src/chat/`](src/chat/) | The `cluaiz run` chat, a provider's model, a running server as the model |
| [`src/cli/`](src/cli/) | One file per command, argument parsing and help ([README](src/cli/README.md)) |
| [`src/core/`](src/core/) | Code shared by the commands |
| [`src/ui/`](src/ui/) | Terminal output helpers |

More detail: [src/README.md](src/README.md).

While a model loads, `cluaiz run` shows what the load is doing on its loading line (`Loading model:
reading weights 42%`, `making KV cache and compute buffers`, `did not fit; retry 2: …`). A local load
gets the steps from `cluaiz_runtime::load_events` (`ui/spinner.rs`, `Spinner::follow_loads`); through
a running server, from the `models.load` task (`chat/remote.rs`).

## Build

Build it with `cargo build --release -p cmd` (only this package; building the whole workspace
rebuilds the GPU backends). The package is `cmd`; its binary is `cluaiz` (`src/main.rs`). The
ggml / llama libraries are loaded from the program's folder at run time (core and GPU packs).

[`build.rs`](build.rs), on Windows, compiles the program's resources: the icon
(`../assets/logo.ico`), file and product names, the version in Windows' four-number form
(`0.1.0-rc.1` becomes `0.1.0.0`), and a manifest that runs as the invoking user (`asInvoker`, no
elevation). It reruns when the icon, `build.rs` or `Cargo.toml` change; the icon lies outside the
package, so without that a new icon would need a clean build. If the resources fail to compile,
the build prints the error and goes on; the program works without its icon.

## Tests

| Where | Count (2026-11-21) | Run |
|---|---|---|
| `src/agent/` unit tests | 37 | `cargo test --release -p cmd --bin cluaiz -- agent:: --test-threads=1` |
| `src/chat/`, `src/cli/` unit tests | 9, 8 | `cargo test --release -p cmd --bin cluaiz` |
| `tests/agent/tools.rs` | 12 | see [`tests/agent/README.md`](tests/agent/README.md) |
| `tests/agent_keyboard.rs` | 1, ignored | `CLUAIZ_KEYBOARD_HOME=<home> cargo test -p cmd --test agent_keyboard -- --ignored --nocapture` |

`cmd` links every backend, so these tests build llama.cpp and ONNX Runtime; they were not run on
2026-11-21. The CLI commands themselves are exercised in cloud runs (run 8: every local command,
`test/cloud/runs/2026-11-14-run8-linux-l4/SUMMARY.md`).

## Decisions and why

| Decision | Why | Alternative not taken |
|---|---|---|
| One binary for the server, the CLI, the agent and the OS sandbox helper | one file to install and update; the Windows sandbox re-runs this same binary as its helper (`run_helper_if_requested` first in `main.rs`) | separate executables |
| Commands that need a running server (`flow`, `storage`) call it over CEP | the server owns that state; a second process would see a stale copy | open the files directly |

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| `cmd` tests not in CI and not run on 2026-11-21 | they need the native build | a CI job with the CPU backend |
| `cluaiz token create` and `permission ...` do not reach a running server | the server keeps its boot copy (run 29) | see `security/README.md` Pending |
