# Agents crate source (`agents/src/`)

## 1. Scope

All tool logic (skills, plugins, MCP, registry, turn lifecycle, telemetry), the chat memory
store and the project's `.agents/` workspace files live in this crate.

---

## 2. Directory Layout & Subsystems

| Subsystem | Folder | Responsibility |
| :--- | :--- | :--- |
| **Facade** | [`lib.rs`](./lib.rs) | `ToolsEngine` public facade used by the API and the CLI. |
| **Agent turn** | [`turn.rs`](./turn.rs) | One agent turn for every front end (API chat streaming and not, terminal agent), through the `Agent` trait (how the model is asked, which tools exist, how a call is approved, where the user sees it). The model answers; tool calls run and their results join the conversation; it answers again. Ends with an answer without calls, a call the client runs itself, a stop, or the step limit. An unusable answer (a call that cannot be read, or only thinking) is not run: the model is told what was wrong and answers again, at most `MOST_CORRECTIONS` times in a row. |
| **Tool search** | [`tool_search.rs`](./tool_search.rs) | Matches a message to the installed tools (whole words), builds the short tool cards, and runs `load_tool`, `unload_tool`, `find_tools`. |
| **Prompt compiler** | [`prompt_compiler.rs`](./prompt_compiler.rs) | Turns the loaded tools into the function schemas the model sees, within a size budget (the ones that fit the message first), and maps a called function back to its tool. |
| **Permissions** | [`permissions.rs`](./permissions.rs) | The permission types a tool's `package.json` may list, which are sensitive, the user's choices (`config/tool_permissions.json`) and the approval tags. See [docs/engine/tools.md](../../docs/engine/tools.md). |
| **Tool settings** | [`tool_env.rs`](./tool_env.rs) | The values a tool names in `env` (API keys, addresses): the system environment first, then the value saved in `config/tool_settings.json`. A tool's program gets only these and the system basics (`execution/environment.rs`). |
| **Memory** | [`memory/`](./memory/) | Chat event store and notes (`proposed`, `confirmed`, `moved`, …). |
| **Execution** | [`execution/`](./execution/) | Command execution policy; destructive commands are always refused. |
| **Registry** | [`registry/`](./registry/) | Manages `tools_registry.json` (kept in RAM; JSON only, no `.bin` cache). Auto-probes `~/.cluaiz/tools/`. |
| **Installer** | [`installer/`](./installer/) | Downloads, caches, installs, and uninstalls packages from Cluaiz Tools. `plan.rs` builds the install plan first: the tool, its dependencies (`required`, `recommended`, `optional`) and their permissions, each installed, asked or only suggested by the user's agent mode. |
| **Workspace** | [`workspace/`](./workspace/) | A project's agent files: `AGENTS.md` (home, `.agents/`, folder files), and `rules/`, `workflows/`, `skills/` of each `.agents/` (or `.agent/`) folder up to the repository root and `~/.agents/`, with name, description and rule trigger, and the project's memory (`.agents/memory/*.md`, facts saved with `remember` after the user confirms; a newer fact on the same subject supersedes the older, both kept). One index, built from the files each time; `matching.rs` finds the items that fit a message (the `tool_search` matcher) or a file (glob rules). Used by the terminal agent and by the API chat (setting `project_files`). Files: `mod.rs` (the index: `Kind`, `Trigger`, `Item`), `instructions.rs` (the AGENTS.md files, with `CLAUDE.md` / `GEMINI.md` as alternatives, sharing one 8,000-character budget that the nearest files take first), `prompt.rs` (the workspace in the system prompt within `budget_for(n_ctx)`: instructions, then an index of rules, workflows, skills and facts the model reads with `read_item`), `matching.rs`, `memory.rs`, `tests.rs`. |
| **Skills** | [`skills/`](./skills/) | `parser.rs`: a `SKILL.md`'s YAML front matter (`SkillMetadata`) and instruction body (`ParsedSkill`). `instructions.rs`: the parsed instructions of each enabled skill, shared in RAM (`SkillInstructions::shared`, `get`). Which skills a request gets is decided in `tool_search.rs`. |
| **Plugins** | [`plugins/`](./plugins/) | Parses `package.json`. A `.wasm` plugin runs inside the Engine, closed in, with memory and instruction limits; any other runs as a normal child process (no OS isolation). |
| **MCP** | [`mcp/`](./mcp/README.md) | MCP client: long-lived server processes, timeouts, idle stop, crash backoff ([README](./mcp/README.md)). |
| **Lifecycle** | [`lifecycle/`](./lifecycle/) | `session_map.rs`: the tools bound to each chat (`SESSION_TOOL_REGISTRY`) with their turns (`-1` always, `0` this answer, `N` answers); a loaded tool not used for `IDLE_TURNS` (3) answers is unloaded unless its manifest sets `default_turns`. `duration.rs`: counts the turns down after each answer. `ledger.rs`: what the Engine added to each turn (tool hints, calls, results), so the next request renders earlier turns exactly as the model saw them and the KV cache is reused. `grants.rs`: "allow this tool for this chat" after an approval with `remember: "chat"` (60 minutes by default, 24 hours at most), kept as `grant` events in the chat's log so they come back after a restart. |
| **Telemetry** | [`telemetry/`](./telemetry/) | `context_tracker.rs`: the context breakdown of a request (system prompt, tools, messages, answer) for a model and chat. `types.rs`: the breakdown shapes; `apply_measured` replaces the character estimates with the engine's measured `n_ctx` and `n_tokens`, keeping the system and tool shares inside that total. |

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
