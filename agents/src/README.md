# Agents crate source (`agents/src/`)

## 1. Scope

All tool logic (skills, plugins, MCP, registry, turn lifecycle, telemetry), the chat memory
store and the project's `.agents/` workspace files live in this crate.

---

## 2. Directory Layout & Subsystems

| Subsystem | Folder | Responsibility |
| :--- | :--- | :--- |
| **Facade** | [`lib.rs`](./lib.rs) | `ToolsEngine` public facade used by the API and the CLI. |
| **Tool search** | [`tool_search.rs`](./tool_search.rs) | Matches a message to the installed tools (whole words), builds the short tool cards, and runs `load_tool`, `unload_tool`, `find_tools`. |
| **Memory** | [`memory/`](./memory/) | Chat event store and notes (`proposed`, `confirmed`, `moved`, …). |
| **Execution** | [`execution/`](./execution/) | Command execution policy; destructive commands are always refused. |
| **Registry** | [`registry/`](./registry/) | Manages `tools_registry.json` (kept in RAM; JSON only, no `.bin` cache). Auto-probes `~/.cluaiz/tools/`. |
| **Installer** | [`installer/`](./installer/) | Downloads, caches, installs, and uninstalls packages from Cluaiz Tools. |
| **Workspace** | [`workspace/`](./workspace/) | A project's agent files: `AGENTS.md` (home, `.agents/`, folder files), and `rules/`, `workflows/`, `skills/` of each `.agents/` (or `.agent/`) folder up to the repository root and `~/.agents/`, with name, description and rule trigger, and the project's memory (`.agents/memory/*.md`, facts saved with `remember` after the user confirms; a newer fact on the same subject supersedes the older, both kept). One index, built from the files each time; `matching.rs` finds the items that fit a message (the `tool_search` matcher) or a file (glob rules). Used by the terminal agent and by the API chat (`project_files`). |
| **Skills** | [`skills/`](./skills/) | Parses `SKILL.md` front matter and keeps each enabled skill's instructions |
| **Plugins** | [`plugins/`](./plugins/) | Parses `package.json`. A `.wasm` plugin runs inside the Engine, closed in, with memory and instruction limits; any other runs as a normal child process (no OS isolation). |
| **MCP** | [`mcp/`](./mcp/) | Parses `package.json` and handles JSON-RPC 2.0 subprocess communication over stdio. |
| **Lifecycle** | [`lifecycle/`](./lifecycle/) | Tracks session-bound active tools, turn countdowns (`-1`, `0`, `N`), and auto-unloading. |
| **Telemetry** | [`telemetry/`](./telemetry/) | Calculates active token breakdowns, lazy loading savings, and KV-cache VRAM allocation. |

---

## 3. Standardized Filesystem Root (`~/.cluaiz/`)

```
~/.cluaiz/
├── config/
│   ├── model_registry.json              <── Master record for GGUF & ONNX models
│   └── tools_registry.json              <── Master record for all installed tools (JSON only)
└── tools/                               <── Master Tools domain folder
    ├── skills/                          <── Installed skills
    │   └── frontend-dev/SKILL.md
    ├── plugins/                         <── Installed plugin programs
    │   └── cluaiz-search/<program> & package.json
    └── mcp/                             <── Installed MCP Servers
        └── sqlite-bridge/package.json
```
