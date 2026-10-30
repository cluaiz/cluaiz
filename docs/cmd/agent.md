# `cluaiz agent`: a coding agent in your terminal

`cluaiz agent` runs a local model as an agent in a folder on your computer. You describe a task in
plain words; the model reads files, searches them, edits them and runs commands to do it, and tells
you what it found or changed. Everything runs on your machine with the models you installed: no
account, no cloud, no data leaves the computer.

```text
>>> calc.py prints the wrong average. Find the bug, fix it, and run it to check.
● Read calc.py
  ⎿ 5 lines
Found it: line 2 adds 1 after dividing.
● Edit calc.py
    -     return sum(numbers) / len(numbers) + 1
    +     return sum(numbers) / len(numbers)
  Allow? [y]es / [n]o / [a]lways this session: y
  ⎿ 1 replaced
● Run python calc.py
  ⎿ exit 0, 1 lines of output
Fixed. It now prints 4.0, the average of [2, 4, 6].
```

- [Start](#start)
- [What the agent can do](#what-the-agent-can-do)
- [What it asks before doing](#what-it-asks-before-doing)
- [Reading the screen](#reading-the-screen)
- [Commands inside the agent](#commands-inside-the-agent)
- [One task from a script](#one-task-from-a-script)
- [Project notes: AGENTS.md](#project-notes-agentsmd)
- [Choosing a model and a context size](#choosing-a-model-and-a-context-size)
- [All options](#all-options)
- [Limits](#limits)
- [Troubleshooting](#troubleshooting)
- [How it works](#how-it-works)

## Start

You need one installed chat model (`cluaiz pull <model>`, see `cluaiz list`).

```sh
cd path/to/your/project
cluaiz agent -m qwen3.8-4b-distill-gguf-Q4_K_M
```

The agent works in the folder you start it from. To work somewhere else without changing folder,
pass `--dir`:

```sh
cluaiz agent -m qwen3.8-4b-distill-gguf-Q4_K_M --dir D:\projects\website
```

Set a default chat model once and you can leave out `-m`:

```sh
cluaiz model set-chat qwen3.8-4b-distill-gguf-Q4_K_M
cluaiz agent
```

With a default chat model set, plain `cluaiz` (no command) in a terminal opens the agent in the
current folder. Without one, it prints the help.

An interactive session opens in the same terminal tab, with the actual model, working folder,
context budget, transcript and bottom input. Type `/bye` or press Ctrl+Q to restore the shell.
No new terminal application is installed. Use `cluaiz agent --ui plain` for the previous `>>>`
interface. A one-shot task or redirected output uses plain mode automatically.

## What the agent can do

The tools are built into `cluaiz` itself. Nothing is downloaded for them, and they are not the
skills, plugins or MCP servers in `~/.cluaiz/tools`.

| Tool | What it does | Asks first? |
| :--- | :--- | :--- |
| `read_file` | Reads a text file with line numbers, 400 lines at a time (the model can ask for more with an offset) | Only outside the working folder |
| `list_dir` | Lists a folder; sub-folders end with `/` | Only outside the working folder |
| `search` | Searches the text of files for a regular expression, optionally only in files like `*.rs`. Skips `.git`, `target`, `node_modules`, `.venv`, `venv`, `__pycache__`, `dist`, `build`, `.next`, binary files and files over 2 MB | Only outside the working folder |
| `write_file` | Creates a file, or replaces all of its content | Always |
| `edit_file` | Replaces an exact piece of text in a file, once (or every occurrence when asked). Works with files that use Windows line breaks | Always |
| `run_command` | Runs one command line in the working folder: PowerShell on Windows, `sh` on Linux and macOS. Stops it after 120 seconds unless the model asks for longer (at most 600) | Unless read-only, see below |

So the agent can, for example:

- explain a project: list it, read the main files, search for where something is defined;
- find and fix a bug, then run the program or the tests to check the fix;
- write a new script or file and run it;
- run builds, tests, `git status` / `git diff`, and read their output;
- change many files: it edits them one by one, each one asked separately (or allowed for the
  session with `a`).

It cannot see your screen, open windows, use the internet by itself (only through a command you
allow, such as `curl`), or remember anything after you exit.

## What it asks before doing

The agent never changes anything without your answer, unless you allow it.

```text
● Create hello.txt
    + hi
  Allow? [y]es / [n]o / [a]lways this session:
```

| Answer | Effect |
| :--- | :--- |
| `y` | This call runs |
| `n`, empty answer or Esc in the UI | This call does not run |
| `a` | This call runs, and every later call of the same tool runs without asking until you exit |
| Other text | Nothing executes; the model receives your instructions with their original case |

In the terminal UI the prompt shows the resolved path or full command, and the proposed file
change. PgUp/PgDn read a long operation; allow is blocked until its last line is visible. Your
draft is preserved while answering the prompt. Closing the UI denies a pending request.

What runs without asking:

- reading, listing and searching inside the working folder;
- commands that only look at things, recognised by how they start: `git status`, `git log`,
  `git diff`, `ls`, `dir`, `cat`, `type`, `Get-ChildItem`, `Get-Content`, `Select-String`, `grep`,
  `rg`, `findstr`, `cargo check`, `cargo test`, `npm test`, `pytest`, `go test`, `whoami`,
  `systeminfo` and similar.

What always asks:

- creating, replacing or editing a file;
- every other command (installing, deleting, moving, building with side effects, ...);
- a "read-only" command that also chains, pipes or redirects: `ls; del x`, `echo hi > file`,
  `dir | Remove-Item`, `cat $(...)` all ask, because only the start of the line was read-only;
- reading, listing or searching outside the working folder.

What is always refused, even with `--yes`: disk and system wipes and similar destructive patterns
(`rm -rf /`, `format c:`, `mkfs`, `dd` to a disk, `shutdown`, `reboot`, fork bombs and others). This
is the same policy the Engine's terminal tool uses.

**Allowing everything.** `cluaiz agent --yes` (or `/yes` inside the agent) runs every change and
command without asking. Use it only in a folder you can restore (for example a git repository with
everything committed). `/ask` turns the questions back on.

**No terminal, no questions.** When the agent cannot ask you (its input is piped from a file or
another program), a call that needs asking is refused and the model is told so, unless you passed
`--yes`.

Commands that run without asking get an environment with secret variables (API keys, tokens)
removed. Commands you allowed run with your normal environment.

## Reading the screen

The interactive screen keeps model words, thinking (unless hidden), tool names, proposed changes
and actual results in stable entries. Failed commands show their exit code as an error. You can
type while generation runs; one further message can wait in the queue. Esc requests cancellation
and clears that queued message. It does not roll back a completed file change or external command.
Commands currently return stdout/stderr when they finish, rather than streaming each output line.

Type `/` for the existing command menu. Arrows choose; Tab inserts; Enter selects a command.
Type `@` to suggest files in the working folder. A filename selection only inserts its path; it
does not submit, read the file, or grant access. Explicit content access goes through the file tool.
Suggestions do not list directories outside the workspace, including symlink targets outside it.

F2 or `/theme` previews Auto, Dark, Light, High contrast and No color. Enter applies the theme for
this session; Esc cancels the preview. Alt+Enter, Shift+Enter (when delivered by the terminal) or
Ctrl+J insert a newline. PgUp/PgDn scroll output; Ctrl+End returns to the latest entry. Ctrl+Q exits.
The transcript is a bounded in-memory view; an eviction notice is shown if older entries are removed.

The examples below describe **plain mode**. Rich Markdown styling is not yet shared with the
interactive renderer; the interactive view currently retains the source Markdown as readable text.

Every tool call is one row, with a one-line result below it:

```text
● Search fn main in .
  ⎿ 3 matches
● Run cargo build
  ⎿ exit 0, 12 lines of output
```

A change shows the lines it removes (`-`, red) and adds (`+`, green) before it asks.

While the model is working and nothing is printed, a status line at the bottom says what it is
doing. It disappears as soon as text arrives:

```text
⠹ Reading the prompt  2s                    the model is reading your message and the files so far
⠼ Writing  412 tokens  9s                   it is thinking or writing (also when thinking is hidden)
⠦ write_file shapes.py  1840 chars  21s     it is writing a tool call: which tool, which file, how much
```

The model's own words are drawn with their Markdown formatting: headings, **bold**, *italic*,
`code`, lists, quotes, tables lined up in columns, code blocks with a `│` gutter, and simple math
(`$3 \times 3$` shows as `3 × 3`). A model that thinks first shows its thinking dimmed between
`Thinking...` and `...done thinking.`; `--hide-thinking` hides it.

Only the rows marked `●` are things that really happened. If the model says it ran or read
something and there is no row for it, it did not.

## Commands inside the agent

| Command | What it does |
| :--- | :--- |
| `/yes` | Allow every change and command for the rest of the session |
| `/ask` | Ask again before each change and command |
| `/clear` | Forget the conversation (the folder and the model stay) |
| `/bye`, `/exit` | Leave (Ctrl+Q in the UI; Ctrl+D in plain mode) |
| `/?`, `/help` | Show these commands |
| `/theme` | Open the terminal UI theme picker |

In plain mode, start and end a message with `"""` to type several lines. Ctrl+C stops the answer being written, or
the command being run, and leaves you at the prompt; at the prompt it exits.

## One task from a script

Give the task on the command line and the agent does it, prints the result and exits:

```sh
cluaiz agent "list the TODO comments in src and say which file has the most"
cluaiz agent -p "run the tests and summarise the failures"
cat task.txt | cluaiz agent --yes            # the task comes from stdin
cluaiz agent -i "read README.md"             # do the task, then keep chatting
```

Without `--yes`, a one-shot task can still read, search and run read-only commands. Anything that
would need asking is refused when there is no terminal to ask.

The answer goes to stdout and the tool rows and status to stderr, so `cluaiz agent "..." > answer.md`
keeps only the answer.

## Project notes: AGENTS.md

If the working folder has an `AGENTS.md` file, its first 8,000 characters are added to the agent's
instructions. Use it to tell the agent how the project is built and tested, which folders to leave
alone, or your style rules. `--system-prompt "..."` adds instructions for one run.

The agent's own instructions tell the model the folder, the operating system and the shell, to use
the tools instead of guessing, never to claim it did something it did not, to read a file before
editing it, and to end with a short summary.

## Choosing a model and a context size

The agent uses the same model loading, planner and settings as `cluaiz run`
(`~/.cluaiz/config/settings.json`, and every load, sampling and thinking flag of `cluaiz run`).

- **Use a model made for tool calls.** Models whose chat template supports tools (Qwen3, gpt-oss,
  Llama 3.1+, Mistral and similar) call tools in their own format. Others get the tools as text in
  the instructions and must write `<tool_call>` blocks themselves, which small models often get
  wrong. Larger models follow the rules better; a 4B model works for small tasks but sometimes
  answers with code instead of writing the file, or guesses instead of looking.
- **Context size.** The conversation, the instructions, the tool descriptions and every file the
  agent read must fit in the model's context. The planner picks the size from your free memory; if
  another program (for example `cluaiz serve` with a model loaded) is using the GPU, the agent may
  get only 4,096 tokens, which fills after a few files. Close other model processes, or ask for
  more with `-c 16384` (part of the model may then run from RAM, which is slower).
- **When the context is full** the agent does not stop: it leaves out the oldest earlier request,
  then (if still needed) replaces the oldest long tool result with a short note, and asks the model
  again. A dim line says so. The model may then need to read a file again.

## All options

```text
cluaiz agent [OPTIONS] [TASK]...
```

| Option | Meaning |
| :--- | :--- |
| `TASK...`, `-p, --prompt <TEXT>` | Task to do, then exit. No task: chat. Piped stdin is the task |
| `-m, --model <MODEL>` | Installed model id or a `.gguf` path (default: the active chat model) |
| `-C, --dir <PATH>` | Folder to work in (default: the current folder) |
| `-y, --yes` | Allow every change and command without asking (destructive commands stay refused) |
| `-i, --interactive` | Keep chatting after the task |
| `--ui auto/plain/tui` | Auto selects the UI for interactive sessions; plain preserves line output; tui requires an interactive terminal |
| `--system-prompt <TEXT>` | Added to the agent's instructions |
| `-n, --n-predict <N>` | Most tokens per answer |
| `-v`, `-vv` | Detailed file logging; console logs in plain mode. UI mode suppresses console tracing so logs do not overwrite input |
| Load flags | `-c`, `-ngl`, `--n-cpu-moe`, `-ctk`, `-ctv`, `-fa`, `-b`, `-ub`, `-t`, `-tb`, `--mmap`, `--no-mmap`, `-nkvo`, `--no-op-offload`, `--vram-buffer` (as `cluaiz run`) |
| Sampling flags | `--temp`, `--top-k`, `--top-p`, `--min-p`, `--repeat-penalty`, `--presence-penalty`, `--frequency-penalty`, `-s` |
| Thinking flags | `--think on/off/auto`, `--reasoning-effort`, `--reasoning-budget`, `--reasoning-budget-message`, `--hide-thinking` |

`cluaiz agent --help` prints the full list.

## Limits

- The skills, plugins and MCP servers you installed (`cluaiz skill / plugin / mcp install`) are not
  offered to the terminal agent yet. They work through the API (`/v1/chat/completions` with tools)
  and the App.
- A request stops after 40 tool calls; say how to go on and it continues.
- The conversation is not saved: closing the agent forgets it.
- No model switch inside the agent: start it again with another `-m`.
- The API and CLI have not yet been moved to one shared agent controller. This terminal frontend
  calls the existing CLI loop; it does not create a second CLI tool executor or load another host.
- Durable sessions, daemon attachment, task views, shared Markdown styling and checkpoints/undo
  remain later implementation stages. Native keyboard/font behavior on macOS/Linux is unverified.
- `search` is a regular expression over file text; to find files by name the model runs a command
  such as `Get-ChildItem -Recurse -Filter *.py` or `find . -name '*.py'`.

## Troubleshooting

| What you see | Why, and what to do |
| :--- | :--- |
| `no model given and no active chat model` | Pass `-m <model>` or run `cluaiz model set-chat <model>` |
| The model prints code and says "save this as ..." | It did not call `write_file`. Ask it directly ("use write_file to create it"), or use a larger or tool-trained model |
| The model says it ran or read something with no `●` row | It did not; it guessed. Ask it to actually run or read it |
| It creates a new file instead of finding yours | The file is not in the working folder. Start the agent in the right folder or pass `--dir` |
| `Context full: the oldest part of the conversation was left out` | The context is small; see [Choosing a model and a context size](#choosing-a-model-and-a-context-size) |
| `request (N tokens) exceeds the available context size` | Even a single message plus the instructions did not fit: start with a bigger `-c`, or `/clear` |
| `not allowed: there is no terminal to ask the user` | The agent ran from a script or pipe; pass `--yes` if the task needs changes |
| `refused: Destructive command pattern ...` | The command matched the always-refused list; do that step yourself |
| Nothing on screen for a while, no status line | stdout or stderr is not a terminal (piped), so no status is drawn; the model is still working |

`cluaiz agent -v` shows the Engine's log on stderr, including how the tools were offered to the
model; the full log is always in `~/.cluaiz/cluaiz_Core.log`.

## How it works

1. Your task, the conversation so far and the agent's instructions are sent to the loaded model
   with the six tool descriptions, the same way an API request's `tools` are sent.
2. The model's chat template renders the tools in its own format. If the template cannot, the tools
   go into the instructions as text and calls are read as `<tool_call>` blocks.
3. The Engine streams the model's words to the screen. A tool call is held back until the model
   finishes it; meanwhile the status line shows its name and target.
4. `cluaiz` runs each call (asking you first when needed), prints its row, and sends the result back
   to the model as a tool message.
5. Steps 1 to 4 repeat until the model answers without calling a tool.

Calls and results are kept in the conversation in the Engine's own form, the same as the API keeps
them, so the CLI and the API treat a conversation with tools the same way.
