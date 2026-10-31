# `cmd/src`: the `cluaiz` command-line interface

A terminal program: interactive `agent` sessions use a same-tab terminal UI; scripts and `--ui plain`
retain line-oriented output. No desktop shell or second executable is required. Load and sampling flags use
llama.cpp's names (`llama-cli`, `llama-bench`); chat commands follow `ollama run`.

## Commands

| Command | What it does |
| --- | --- |
| `cluaiz run [model] ["prompt"]` | Chat, or answer one prompt and exit. `model` = installed id, a `.gguf` path, or an id to download. No model = the active chat model. |
| `cluaiz agent ["task"]` | Coding agent in the current folder (`--dir`): reads, searches and edits files, runs commands. Bare `cluaiz` on a terminal opens it when there is an active chat model. |
| `cluaiz show <model>` | GGUF details, thinking tags, and the plan the model would get now (`-v`: full planner banner, all metadata, tensors) |
| `cluaiz list` | Installed models (`*` = active chat model) |
| `cluaiz bench <model>` | llama-bench's test (`pp512`, `tg128`, mean ± sd); `-p`, `-n`, `-r`, `-o md` or `-o json` |
| `cluaiz config list / get / set / reset / path` | Engine settings in `settings.json` (same merge rules as the API) |
| `cluaiz serve`, `cluaiz ps` | API server, and the model it has loaded |
| `cluaiz status`, `cluaiz calibrate` | Hardware the planner sees; re-scan it after a hardware or driver change |
| `cluaiz pull <id>`, `cluaiz rm <id>` | Download + register a model (ends with the command to run it); delete it (folder + registry, exact id) |
| `cluaiz dev-sync`, `skill/plugin/mcp install` | Development sync and component installation |

`-v` on any command shows engine logs on stderr (`-vv` debug). The full log is always in
`~/.cluaiz/cluaiz_Core.log` (appended; restarted after 20 MiB).

## `agent`

```sh
cluaiz agent                                    # chat with the agent in this folder
cluaiz agent "fix the failing test in calc.py"  # one task, then exit
cluaiz agent -m qwen3 --dir ../app --yes "..."  # model, folder, no questions
```

- Tools (`agent/tools.rs`): `read_file`, `list_dir`, `search` (regex over the folder, skipping
  `.git`, `target`, `node_modules`, ...), `write_file`, `edit_file` (exact text, once unless
  `replace_all`), `run_command` (PowerShell on Windows, `sh` elsewhere).
- `agent/terminal.rs` owns one UI worker and bounded event/input queues over the existing turn
  loop. `display.rs` adapts that loop to either the old printer or terminal events; `terminal_state.rs`
  projects them into stable entries. `chat/output.rs` is the shared generation output boundary.
- `/` suggestions use `agent/commands.rs`; `@` filenames use the existing canonical CEP roots
  resolver and bounded directory enumeration off the screen thread. Paths outside the workspace
  are not passively listed. Choosing a filename inserts a reference; the existing read tool performs
  any later content access. Theme/approval/completion widgets live in the `terminal` library.
- Each call is one row (`● Read src/main.rs`, `⎿ 120 lines`); a change shows its `-` / `+` lines.
- While nothing is printed, a status line on stderr says what the model is doing: reading the
  prompt, tokens written (hidden thinking), or the call being written with its path or command
  (`⠦ write_file shapes.py  1840 chars  21s`). The Engine holds a call back until generation ends;
  `cluaiz_core::live` carries its progress. `cluaiz run` shows the same line.
- A full context does not end the conversation: the oldest earlier request is left out (then the
  oldest long tool result of this one) and the answer is asked again.
- Reads inside the folder run at once. A read outside it, every write or edit, and every command
  that is not read-only ask `[y]es / [n]o / [a]lways this session`. `--yes` or `/yes` allows them all.
  Commands go through `cluaiz_agents::ExecutionPolicy`: destructive patterns are refused even then.
  The policy reads the start of a line, so a read-only start that chains, pipes or redirects
  (`ls; del x`, `echo hi > file`) asks too. User guide: [docs/cmd/agent.md](../../docs/cmd/agent.md).
  With no terminal to ask (piped stdin) a call that needs asking is refused unless `--yes`.
- The tools are sent the way an API request's `tools` are: the model's own template renders them
  (or the inline `<tool_call>` form when it cannot), and calls come back as `<tool_call>` blocks
  (`cluaiz_core::tool_calls`). The conversation keeps calls and results in the Engine's form.
- The system prompt names the folder, the OS and the shell, and adds the folder's `AGENTS.md`
  (first 8,000 characters) and `--system-prompt`. A request stops after 40 tool steps.
- Installed skills, plugins and MCP servers (enabled in `~/.cluaiz/config/tools_registry.json`)
  are offered next to the built-ins: schemas from `ToolsEngine::compile_prompt_tools` (at most
  12,000 characters; the rest is left out with a notice), skill instructions added to the system
  prompt, calls resolved with `resolve_function_target` and run by `execute_tool_by_name` in the
  working folder, each asked first like a command. The agent loop itself is still the CLI's own.

## `run`

`pull` and automatic installation from `run` do not require website login. A short catalogue ID uses one resolver request; failures explain whether the ID is missing, the request was limited, the client was rejected, or the registry is unavailable. To bypass the website, supply the full HF repository (`owner/repository`, optionally `:QUANT`). Existing local models and local catalogue templates remain available. Command copy and installation do not send a separate counting event.

```sh
cluaiz run qwen3-4b-gguf-Q4_K_M "Why is the sky blue?"     # one answer
cat notes.txt | cluaiz run qwen3 --system-prompt "Summarize" > summary.txt  # stdin is the prompt
cluaiz run ./model.gguf -ngl 20 -c 8192 -fa on --temp 0.6    # llama.cpp flags
cluaiz run qwen3 --think on --reasoning-effort low            # or --reasoning-budget N (llama.cpp)
cluaiz run qwen3                                              # chat
```

- stdout carries only the answer. Thinking is printed (dimmed) only when stdout is a terminal;
  `--hide-thinking` hides it there too. Plan summary and speed go to stderr.
- On a terminal the answer's Markdown is drawn formatted (`chat/markdown.rs`): each line is redrawn
  when it ends (headings, bold, italic, code, links, lists, quotes, simple `$math$`), a table is
  redrawn aligned after its last row, and code blocks get a gutter. Piped output stays the model's
  own Markdown.
- `-v` with a command shows engine logs (`-vv` debug); `cluaiz -v` alone prints the version, as
  `-V` / `--version` do.
- Load flags: `-c`, `-ngl`, `--n-cpu-moe`, `-ctk`, `-ctv`, `-fa`, `-b`, `-ub`, `-t`, `-tb`, `--mmap`,
  `--no-mmap`, `-nkvo`, `--no-op-offload`, `--vram-buffer`. They are applied over `settings.json` exactly
  like an API request's `"llama"` object (`LlamaLoadParams`). A missing flag = settings / planner.
- Sampling: `--temp`, `--top-k`, `--top-p`, `--min-p`, `--repeat-penalty`, `--presence-penalty`,
  `--frequency-penalty`, `-s/--seed`, `-n/--n-predict`, `-sys/--system-prompt`.

### Chat commands

`/set parameter <name> <value>`, `/set system`, `/set think`, `/set effort`, `/set budget-message`,
`/set verbose|quiet`, `/set showthinking|hidethinking`, `/show info|plan|parameters|system`,
`/load <model>`, `/models` (browse / download / delete), `/save <file>`, `/clear`, `/bye`, `/?`.
`"""` starts and ends a multi-line message. Ctrl+C stops an answer; at the prompt it exits.
`cluaiz run <model> -i < messages.txt` runs the chat with one message per stdin line.

## Layout

| Path | Contents |
| --- | --- |
| `main.rs` | clap definitions, logging setup, dispatch |
| `cli/args.rs` | Shared load / sampling / thinking flags and llama.cpp short-flag names (`-ngl`, `-fa`, ...) |
| `cli/models.rs` | Model name → file (path, or the installed registry the API also uses) |
| `cli/keys.rs` | Dotted keys over JSON (`config`, `/set parameter`) |
| `cli/server.rs` | Requests to a running `cluaiz serve` (port + session token) |
| `chat/` | Chat session (`mod.rs`), terminal output (`print.rs`, `markdown.rs`), interactive loop (`repl.rs`) |
| `agent/` | `cluaiz agent`: the tool loop (`mod.rs`) and the built-in tools with their approvals (`tools.rs`) |
| `cli/install.rs` | Download flow shared by `pull`, `run` and the model list: resolve, confirm, `ModelDownloader::install` |
| `core/` | `bootstrapper.rs` (start-up checks, dev-sync), `state.rs` (model list state) |
| `ui/apps/registry/` | Model list with download / delete (unchanged) |

Chat turns are sent with `cluaiz_core::ChatEnvelope` and the same context rules (`trim_history`,
`answer_budget`) as the API's `/v1/chat/completions`, so the CLI and the API behave the same.
