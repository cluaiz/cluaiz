# Run 31: the thinking budget closes gently

Windows 11 (build 26300), Engine 0.1.0 debug build (`cargo build -p cmd`), Qwen3-0.6B Q4_K_M on the
machine's GPU, an isolated `CLUAIZ_HOME` on port 8011. Script: `gentle-close.sh`. Raw answers:
`soft-a.json`, `soft-b.json`, `soft-c.json`; server log lines: `server-log-lines.txt`.

## What changed in the code

Before: at the budget the sampler wrote the end-of-thinking tag at once, mid-sentence, with no text
(`reasoning_budget_message` was empty by default). Now, for `auto`, `low`, `medium` and `high`, a spent
budget closes at the next line end (grace: a quarter of the budget, at most 500 tokens, inside the
room), and the close writes Qwen's early-stopping sentence before the tag (managed prompt
`chat.thinking_close`). A number of tokens and `max` stay exact.

## Result

| Request | Budget | Thinking tokens | Where it closed | Answer |
|---|---|---|---|---|
| a: cubes puzzle | `low` = 2,048 | 2,069 | after a finished line ("(3, 2): 27 + 8 = 35.") | full answer, stopped at `max_tokens` 600 |
| b: odd numbers proof | `low` = 2,048 | 2,124 | after a finished line, conclusion already written | full answer, ended by the model |
| c: cubes puzzle | `300` (exact) | 322 | mid-line ("y = 4: 64 - 1 = 63,"), then the sentence | full answer, ended by the model |

The reported thinking count includes the forced sentence and tag (about 20 tokens). The log shows
`Thinking budget reached; closed at the end of a line` for a and b.

## Not measured

Answer quality with and without the gentle close (needs a scored question set), and a slow CPU.
