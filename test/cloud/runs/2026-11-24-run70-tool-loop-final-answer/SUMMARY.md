# Run 70: final answer without tools at the tool step / repeat limit, before and after (2026-11-24)

## What was tested

With a long prompt and the Engine's own tools, a small model kept calling tools. The API turn stopped at the repeat
limit (4 identical calls in a row, `REPEAT_STOP_THRESHOLD`, `agents/src/turn.rs:127` and `:175-176`) or at
`max_steps` (`agents/src/turn.rs:193`, default 100 from `api/src/handlers/chat/api_turn.rs:19`), and the client got
no answer. The callers before this change: `non_streaming.rs:23` and `agent_loop.rs:163` (`cluaiz_agents::turn::run`),
then for `Ended::StepLimit` only the text before `<tool_call>` of the last generation was returned (commit `afc9900d`),
here always `"\n"`.

The change (owner's "Option B"): `api_turn::run` wraps `cluaiz_agents::turn::run`; at `Ended::StepLimit` it asks the
model once more with `ToolTurns::final_answer` (no `tools`, `tool_choice: "none"`, all tool results kept, managed
prompt `agent.final_answer` as the last user message) and returns that reply as `content`. Not when the request
carried its own `tools` (`client_tools`) or was cancelled. One log line: `stopped at step/repeat limit (max_steps=N):
asking once more for a final answer without tools`.

## Setup

| Item | Value |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 Laptop 4 GB, shared with other work (free VRAM per request in `gpu_before` of `results.jsonl`: 447 to 2,566 MiB) |
| Before binary | `cargo build -p cmd` debug of `1dcf2212` (run 66 summary commit before the history rewrite; now `730c3f79`, which differs from it only in two scrubbed test log files, no Engine code), SHA-256 `f353bdb12ce4d39c671ae8e606a5bd6f1eed8ac23126c18762dc39e890a5efaf` |
| After binary | the same tree plus this change (committed as `eb920d61`) and another agent's then-uncommitted `engine/llama/src/native/stream.rs` change (now commit `e654894f`, `response_format` without tools; no request here sends `response_format`), SHA-256 `bdf2ee4ad70e86b78c64a930309dbb6a320ee6e51742beca01f72209664d3f0c` |
| Server | copies of each exe + DLLs in the scratchpad, `cluaiz serve`, isolated `CLUAIZ_HOME` (new), `CLUAIZ_PORT=8014`, `CLUAIZ_HOST=127.0.0.1`, auth settings untouched; one fresh server per set |
| Tools in the home | `plugin install math --yes`, `mcp install filesystem --yes` (run 64's way; `raw/install_*.txt`) |
| Model | `qwen3.5-0.8b-gguf-Q4_K_M` (run 52 / 55's model). Qwen3-0.6B Q4_K_M was tried first: it answered all 3 first-look prompts at once with no tool call, so it was not used (that first look's files were not kept) |
| Request | run 52 `long_input`: `"The marker is N. " + "word " * (1500 + 37 i) + " What is the marker?"`, N = 4917 + 111 i, i = 0..9 (i = 0 is run 52's prompt), `temperature: 0`, `think_mode: "off"`, `max_tokens: 48`, no `session_id` |
| Script | `loop_probe.py` (one set per call), `extra_checks.sh` (max_steps / client tools / streaming), `summarize.py` (tables) |

"Real answer" = HTTP 200, non-empty `content` (streaming: the joined content deltas) without `<tool_call>`, and no
client `tool_calls`. Steps = `Step N: generating again` log lines of the request (generations = steps + 1).

## Results

| Set | n | Real answer | Marker in answer | Ended at a limit | Final no-tools turn | Steps (sum / max) | Median s | Total s |
|---|---|---|---|---|---|---|---|---|
| before (first set) | 10 | 0 | 0 | 10 | 0 | 50 / 5 | 203.4 | 2062.5 |
| before (second set) | 10 | 0 | 0 | 8 | 0 | 155 / 99 | 144.9 | 1826.0 |
| after | 10 | **8** | 6 | 8 | 8 | 155 / 99 | 146.7 | 1913.7 |
| before, `max_steps: 2` | 5 | 0 | 0 | 5 | 0 | 5 / 1 | 5.8 | 52.4 |
| after, `max_steps: 2` | 5 | **5** | 3 | 5 | 5 | 5 / 1 | 7.1 | 44.6 |
| before, `stream: true` | 3 | 0 | 0 | 3 | 0 | 18 / 6 | 140.9 | 433.6 |
| after, `stream: true` | 3 | **3** | 2 | 3 | 3 | 16 / 6 | 139.4 | 430.4 |
| before, client `tools` (`lookup_marker`) | 3 | 0 (3 client `tool_calls`) | 0 | 0 | 0 | 0 / 0 | 2.9 | 10.0 |
| after, client `tools` | 3 | 0 (3 client `tool_calls`) | 0 | 0 | 0 | 0 / 0 | 3.0 | 10.3 |

The second before set and the after set took the same tool path request by request (same step count in all 10,
`raw/tables.md`), so they are the clean pair. The first before set ran when less VRAM was free (context 8,192 instead
of 16,384, prefill about 130 t/s instead of about 6,000 t/s) and the model took a shorter path (repeat limit at 5
steps every time); it is kept as found, not used for timing.

- Every turn that reached a limit ended with an answer after the change: 8 / 8, 5 / 5, 3 / 3. No final answer wrote a
  call (`final_still_called` 0).
- Both limits were reached: request i = 7 ran 100 generations (`max_steps`), the others the repeat limit.
- Answers are the model's own; 2 of the 8 say "The marker is filesystem", a wrong answer from the 0.8B model.
  All final answers ended at `max_tokens` 48 (`finish_reason: "length"`).
- i = 3 and i = 9 failed the same way before and after: not at a limit; the 8th generation was cut by `max_tokens`
  inside a `write_file` call, and that half call comes back as `content` (Pending).
- Client tools: identical before and after (the model called the client function first; the turn never reached a
  limit, so the skip for client tools was not exercised live).
- The final turn's log has no `[NativeTools] Format ... grammar on` line, unlike every tool step: no tool schemas
  reached the template.

## Time

The final no-tools answer took 1.0 to 2.9 s (log line to the end of that generation; 13 requests measured). Its prompt
has no tool schemas, so it starts differently and is processed whole: 3,784 to 11,339 tokens, 0 from the cache, at
4,800 to 7,600 t/s. Request times are otherwise unchanged within noise (pair medians 144.9 s and 146.7 s). Most of
each looping request is not the model: the first `filesystem` call needs approval, nobody answers, and the Engine waits
120.0 s (all 10 requests of the second before set, `Confirmation timed out`); later calls in the turn are refused at
once.

## What this does not prove

- Other models, thinking on, other prompts, larger `max_tokens`, a provider model.
- A request with client `tools` that reaches a limit (the code skips the final answer there; not reached live).
- Image or audio requests (pivot prompt) and templates without native tools (inline tools stay in the system text).
- Answer quality: the final answers were checked for being an answer and for the marker number only.

## Files

`raw/<set>/results.jsonl` (one row per request), `replies.jsonl` (full reply), `request_logs.jsonl` (engine log lines
of that request), `engine.log`, `summary.jsonl`; `raw/raw_*.out` (console); `raw/explore_first_look/` (first 3
requests on the before binary with the older probe, which mixed filler sentences; only the `word` prompt looped);
`raw/tables.md`. User name replaced by `<user>`; the session token is read from the home and is not in any file.

Rerun: build both binaries with `cargo build -p cmd`, copy each exe with its DLLs, make a new home with the two
installs above and the model registered, then
`python -B loop_probe.py --exe <exe> --home <home> --port 8014 --out <dir> --model qwen3.5-0.8b-gguf-Q4_K_M --case marker --n 10`
and `sh extra_checks.sh` (edit `W` first).

## Pending

| Item | Needs |
|---|---|
| Half a tool call at `max_tokens` returned as `content` (i = 3, 9) | handle a cut call at `finish_reason: "length"` in `api_turn.rs` |
| Client tools + limit, media, inline tools, terminal agent | the runs or code listed in `api/src/handlers/chat/README.md` Pending |
| 120 s approval wait per turn when no client answers | owner decision whether API requests without an approval UI should wait at all |
