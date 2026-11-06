# The `cluaiz` program (`cmd/`)

The command line of the Engine. `cluaiz --help` lists every command; `cluaiz <command> -h` explains one.
The full references are [docs/cmd/commands_reference.md](../docs/cmd/commands_reference.md) (commands),
[docs/cmd/agent.md](../docs/cmd/agent.md) (coding agent) and [docs/cmd/chat.md](../docs/cmd/chat.md) (chat).

| Commands | What they do |
|---|---|
| `agent`, `run` | Coding agent in the current folder; chat with a model or answer one prompt |
| `pull`, `list`, `show`, `rm`, `model`, `ps` | Download, list, inspect and remove models; choose the defaults; see what the server has loaded |
| `serve`, `mcp-server`, `token`, `permission` | HTTP server, MCP over stdio, API tokens, `permission.json` |
| `status`, `calibrate`, `bench` | The hardware the planner sees, a new hardware scan, speed measurements |
| `skill`, `plugin`, `mcp` | Installed tools |
| `ingest`, `storage` | Document ingest; temporary storage of the running server |
| `config`, `self`, `dev-sync` | `settings.json`; update or uninstall; copy development artifacts |

## Code

| Folder | Contents |
|---|---|
| [`src/agent/`](src/agent/) | The coding agent: tools, permissions, sessions, checkpoints, its terminal screen |
| [`src/chat/`](src/chat/) | The `cluaiz run` chat |
| [`src/cli/`](src/cli/) | One file per command, argument parsing and help |
| [`src/core/`](src/core/) | Code shared by the commands |
| [`src/ui/`](src/ui/) | Terminal output helpers |

Build it with `cargo build -p cmd`.
