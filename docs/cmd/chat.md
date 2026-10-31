# `cluaiz run`: chat with a model in the terminal

`cluaiz run` loads a model and either answers one prompt or opens a chat. It is a plain chat: the
model only writes text. To let a model read and change files or run commands, use
[`cluaiz agent`](agent.md).

```sh
cluaiz run qwen3.8-4b-distill-gguf-Q4_K_M                         # chat
cluaiz run qwen3.8-4b-distill-gguf-Q4_K_M "Why is the sky blue?"  # one answer, then exit
cat notes.txt | cluaiz run qwen3 --system-prompt "Summarize" > summary.md
cluaiz run ./model.gguf -ngl 20 -c 8192 -fa on --temp 0.6         # llama.cpp flags
```

The model can be an installed id (`cluaiz list`), a path to a `.gguf` file, or a registry / Hugging
Face id, which is downloaded first. With no model, the active chat model is used
(`cluaiz model set-chat <id>`).

## Using a running `cluaiz serve`

If `cluaiz serve` is running, `cluaiz run` uses the model loaded there instead of loading it again
(`Using <model> in the running server at ...`). The server's settings decide placement and context.
`--local`, or any load flag (`-c`, `-ngl`, ...), loads the model in this process instead.

## What you see

**Formatted answers.** On a terminal the answer's Markdown is drawn as it streams: each line is
redrawn formatted when it ends.

| The model writes | You see |
| :--- | :--- |
| `## Heading` | the heading in bold colour, without `##` |
| `**bold**`, `*italic*`, `~~struck~~` | bold, italic, struck text |
| `` `code` `` | the code in colour |
| `- item`, `- [ ] task`, `- [x] done` | `• item`, `☐ task`, `☑ done` |
| `> quote` | `│ quote` in italics |
| `[text](https://...)` | the text underlined, the address dimmed after it |
| a table (`\| a \| b \|`) | the table lined up in columns once its last row has arrived; a table wider than the window is shown one row per line |
| ```` ```python ```` code block | the language name, then the code behind a `│` gutter |
| `$3 \times 3 + \frac{2}{4}$` | `3 × 3 + 2/4` (common symbols: × ÷ · ± ≤ ≥ ≠ ≈ ∞ √ π →) |

A `*` or `_` inside a word stays as it is, so `2*3*4` and `snake_case` are not turned into italics,
and `$5 and $10` stays a price.

**Thinking.** A model that thinks before answering shows its thinking dimmed between `Thinking...`
and `...done thinking.` (as in ollama). `--hide-thinking` or `/set hidethinking` hides it.

**Status line.** When nothing is printed for a moment, a line at the bottom says what is happening,
and it disappears when text arrives:

```text
⠹ Reading the prompt  3s     the model is reading a long prompt or file
⠼ Writing  412 tokens  9s    it is generating (for example thinking that is hidden)
```

**Plan and speed.** At the start of a chat the placement is printed (GPU / RAM split, context size,
memory). `-v` or `/set verbose` adds the speed after each answer (prompt and generation tokens per
second) and the full planner report.

## Output to files and other programs

Everything except the answer goes to stderr: the plan, the speed, the status line. stdout carries
only the answer, and when stdout is not a terminal it is the model's own Markdown, untouched, with
no thinking:

```sh
cluaiz run qwen3 "Write a README for a todo app" > README.md
```

Colours are off when stdout is not a terminal, and when `NO_COLOR` is set.

## Chat commands

| Command | What it does |
| :--- | :--- |
| `/set parameter <name> <value>` | `temp`, `top_k`, `top_p`, `min_p`, `repeat_penalty`, `presence_penalty`, `frequency_penalty`, `seed`, `n_predict` (`default` unsets) |
| `/set system <text>` | System prompt |
| `/set think <on\|off\|auto>` | Thinking of reasoning models |
| `/set effort <level\|N>` | Thinking budget: `auto`, `low`, `medium`, `high`, `max` or tokens |
| `/set budget-message <text>` | Text written when the thinking budget runs out |
| `/set verbose`, `/set quiet` | Show or hide the speed after each answer |
| `/set showthinking`, `/set hidethinking` | Show or hide the thinking |
| `/show info\|plan\|parameters\|system` | Model details, the plan, the current values |
| `/load <model>` | Switch to another installed model, keeping the conversation |
| `/models` | Browse, download or delete models |
| `/save <file>` | Save the conversation as JSON |
| `/clear` | Forget the conversation |
| `/bye` | Exit (also Ctrl+D) |
| `/?` | Help |

`"""` starts and ends a message of several lines. Ctrl+C stops an answer; at the prompt it exits.
`cluaiz run <model> -i < messages.txt` runs a chat with one message per line of the file.

## Options

| Option | Meaning |
| :--- | :--- |
| `[MODEL]` | Installed id, `.gguf` path, or an id to download |
| `[WORDS]...`, `-p, --prompt <TEXT>` | Prompt to answer once, then exit (no prompt: piped stdin is the prompt) |
| `-f, --file <PATH>` | Read the prompt from a file |
| `--system-prompt <TEXT>` | System prompt |
| `-n, --n-predict <N>` | Most tokens per answer |
| `-i, --interactive` | Keep chatting after answering the prompt |
| `-a, --all` | When downloading: any files of the repository, not only model formats |
| `-v`, `-vv` | Engine logs on stderr: info (with the full plan) or debug. `~/.cluaiz/cluaiz_Core.log` always has them |
| Load | `-c` (`N`, `auto`, `max`), `-ngl` (`N`, `all`, `auto`), `--n-cpu-moe`, `-ctk`, `-ctv`, `-fa`, `-b`, `-ub`, `-t`, `-tb`, `--mmap`, `--no-mmap`, `-nkvo`, `--no-op-offload`, `--vram-buffer` |
| Sampling | `--temp`, `--top-k`, `--top-p`, `--min-p`, `--repeat-penalty`, `--presence-penalty`, `--frequency-penalty`, `-s` |
| Thinking | `--think`, `--reasoning-effort`, `--reasoning-budget`, `--reasoning-budget-message`, `--hide-thinking` |

Load and sampling flags use llama.cpp's names and are applied over `settings.json` the same way an
API request's values are; a flag you leave out keeps the setting, then the planner's choice.

## Version and verbose

| You type | You get |
| :--- | :--- |
| `cluaiz -v`, `cluaiz -V`, `cluaiz --version` | The version, e.g. `cluaiz 0.1.1` |
| `cluaiz run <model> -v` | The chat with Engine logs (info) on stderr |
| `cluaiz run <model> -vv` | The chat with debug logs |

`-v` means version only when it is the only thing after `cluaiz`; with a command it means verbose.
