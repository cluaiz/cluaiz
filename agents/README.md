# Cluaiz Agents (`agents/`)

Installed tools (skills, plugins, MCP servers), tool search, chat memory storage, notes and the
project's `.agents/` files.

## Modules

| Module | Job |
|---|---|
| [`registry/`](src/registry/) | `tools_registry.json`: the installed tools and their settings |
| [`installer/`](src/installer/) | Download, install and remove tool packages |
| [`tool_search.rs`](src/tool_search.rs) | Matches a message to the installed tools (whole words) and builds the short tool cards; `load_tool`, `unload_tool`, `find_tools` |
| [`skills/`](src/skills/) | Parses `SKILL.md`; `SkillRouter` matches the registry's trigger words |
| [`plugins/`](src/plugins/) | Plugin manifest and executor. A plugin runs as a normal child process; there is no sandbox |
| [`mcp/`](src/mcp/) | MCP client over stdio (JSON-RPC 2.0), server start and restart |
| [`execution/`](src/execution/) | Command execution policy (destructive commands are always refused) |
| [`lifecycle/`](src/lifecycle/) | Which tools are loaded in a chat, and when they are unloaded |
| [`telemetry/`](src/telemetry/) | Token counts of the loaded tools |
| [`memory/`](src/memory/) | Chat event store, notes (`proposed`, `confirmed`, `moved`, …) |
| [`workspace/`](src/workspace/) | The project's agent files: `AGENTS.md`, `.agents/rules`, `workflows`, `skills`, `memory` |

More detail: [src/README.md](src/README.md).
