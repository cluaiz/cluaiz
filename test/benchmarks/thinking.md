# Thinking budget and Auto thinking

## Answer

- **Gentle close (run 31):** the `low` budget (2,048) closed at a line end (2,069 and 2,124 thinking tokens) with a
  full answer.
- **Auto ceiling (run 30):** the Auto ceiling follows the measured speed; the model used 285-1,749 thinking tokens.

Below: every current run of this test type, newest first, in full.

<a id="run-31"></a>

## Run 31 (2026-11-21)

Windows 11 (build 26300), Engine 0.1.0 debug build (`cargo build -p cmd`), Qwen3-0.6B Q4_K_M on the
machine's GPU, an isolated `CLUAIZ_HOME` on port 8011. Script: `gentle-close.sh`. Raw answers:
`soft-a.json`, `soft-b.json`, `soft-c.json`; server log lines: `server-log-lines.txt`.

#### What changed in the code

Before: at the budget the sampler wrote the end-of-thinking tag at once, mid-sentence, with no text
(`reasoning_budget_message` was empty by default). Now, for `auto`, `low`, `medium` and `high`, a spent
budget closes at the next line end (grace: a quarter of the budget, at most 500 tokens, inside the
room), and the close writes Qwen's early-stopping sentence before the tag (managed prompt
`chat.thinking_close`). A number of tokens and `max` stay exact.

#### Result

| Request | Budget | Thinking tokens | Where it closed | Answer |
|---|---|---|---|---|
| a: cubes puzzle | `low` = 2,048 | 2,069 | after a finished line ("(3, 2): 27 + 8 = 35.") | full answer, stopped at `max_tokens` 600 |
| b: odd numbers proof | `low` = 2,048 | 2,124 | after a finished line, conclusion already written | full answer, ended by the model |
| c: cubes puzzle | `300` (exact) | 322 | mid-line ("y = 4: 64 - 1 = 63,"), then the sentence | full answer, ended by the model |

The reported thinking count includes the forced sentence and tag (about 20 tokens). The log shows
`Thinking budget reached; closed at the end of a line` for a and b.

#### Not measured

Answer quality with and without the gentle close (needs a scored question set), and a slow CPU.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-21-run31-thinking-gentle-close/`](../cloud/runs/2026-11-21-run31-thinking-gentle-close/)

<a id="run-30"></a>

## Run 30 (2026-11-21)

Windows 11 (build 26300), Engine 0.1.0 debug build (`cargo build -p cmd`), Qwen3-0.6B Q4_K_M on the
machine's GPU, 16k context, an isolated `CLUAIZ_HOME` on port 8011 (never the owner's 8000 server).
Script: `auto-effort.sh`. Raw output: `output.txt` (usage lines, then the server log lines).

#### What changed in the code

`reasoning_effort: "auto"` was the same as `"high"` (`engine/core/src/settings.rs`). It is now 180 seconds
of thinking at the loaded model's last measured decode speed, at least 1,024 tokens, at most High's share
of the room, never Max. The first answer after a load has no speed yet and uses High's share.

#### Result

| Request | Ceiling (log) | Speed used | Thinking tokens the model used | Ended by |
|---|---|---|---|---|
| "What is 2+2?" | 11,976 | none yet (first answer) | 285 | the model |
| 17 x 23 with steps | 11,971 | 180.2 t/s | 1,749 | the model |
| train trip length | 11,963 | 173.9 t/s | 1,176 | the model |

180 t/s x 180 s is about 32,400 tokens, more than 75% of the room, so the room share applied, as the
rule says. The model ended every thought on its own, far below the ceiling.

#### Not tested here

A slow machine, where the speed rule lowers the ceiling (5 t/s gives 1,024, 36 t/s gives 6,480). Covered
by unit tests in `engine/core/src/settings_tests.rs`; a CPU-only run is pending (8 GB cloud VM).

Logs, raw results and scripts: [`test/cloud/runs/2026-11-21-run30-auto-thinking-ceiling/`](../cloud/runs/2026-11-21-run30-auto-thinking-ceiling/)

## All logs

| Run | Folder |
|---|---|
| 31 | [`2026-11-21-run31-thinking-gentle-close`](../cloud/runs/2026-11-21-run31-thinking-gentle-close/) |
| 30 | [`2026-11-21-run30-auto-thinking-ceiling`](../cloud/runs/2026-11-21-run30-auto-thinking-ceiling/) |
