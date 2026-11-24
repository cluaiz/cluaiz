# Run 73: tool-loop, cut-call, MTP log and response_format / tool_choice fixes, before and after (2026-11-25)

## Setup

| Item | Value |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 Laptop 4 GB, shared with other agents' work (free VRAM 2.4-2.8 GB at the checks) |
| Before binary | `cargo build -p cmd` debug of `7498b6b4` (the same file as run 70's after binary, SHA-256 `bdf2ee4a...3f0c`) |
| After binaries | one debug build per fix, each hard-linked with its DLLs into a scratch folder and deleted after its tests; SHA-256 in `raw/binaries/*_sha.txt` (`t3after` = cfa597f2, `t2after` = b62e4e3b, `t1after` = d7748fd1, `rfafter` = e72069a0 (the v2 build; v1 results in `raw/rf_v1after_*` were the stream.rs part alone), `f13after` = afb5b8e3 + the first version of bbdd9706, `f3after` = bbdd9706, `f2after` = 99a3000a, `f4after` = 909a2d2f) |
| Server | `cluaiz serve`, isolated `CLUAIZ_HOME` (run 70's config and tools: math plugin, filesystem MCP; no session token copied), `CLUAIZ_PORT=8013`, `CLUAIZ_HOST=127.0.0.1`, one server at a time; auth settings untouched |
| Models | Qwen3.5-0.8B Q4_K_M (cut call, tool loop, terminal agent), Qwen3.5-2B Q4_K_M + mmproj-F16 (MTP log, image cases), Qwen3-0.6B Q4_K_M (run 68 / 69 cases, tool_choice none, schema) |
| Scripts | `mtp_log_check.sh`, `cut_call_probe.py`, `marker_cut_check.sh` + run 70's `loop_probe.py`, `cli_limit_check.sh`, `media_tools_probe.py`, `choice_none_probe.py`, `bad_schema_probe.py`, `count_calls.py`; run 68's `fixcheck.py` and run 69's `edgecheck.py` from their folders |

## Results

| Fix (commit) | Check | Before | After |
|---|---|---|---|
| MTP log line (`cfa597f2`) | `cluaiz bench` Qwen3.5-2B MTP, `CLUAIZ_MTP=off` | `[MTP] Auto: on with the per-answer guard`, no drafter | `[MTP] off by CLUAIZ_MTP (Auto alone: off, its memory would not fit next to this placement)`, no drafter |
| | `CLUAIZ_MTP=on` | `Auto: on ...` + `Drafting up to 3 tokens` | `[MTP] on by CLUAIZ_MTP (Auto alone: ...)` + `Drafting up to 3 tokens` |
| Cut native call not sent (`b62e4e3b`) | client `save_note`, `max_tokens` 40, 5 requests | 5 / 5 client calls with cut JSON arguments, `finish_reason: tool_calls` (streaming 5 / 5 too) | 0 / 5 calls, `finish_reason: length`, empty content (streaming the same); log `1 call(s) cut by the token limit` 5 / 5 |
| | run 70 i = 3, 9 (Engine tools, `max_tokens` 48) | no cut happened (both reached the repeat limit and answered) | identical answers; the run 70 failure did not recur |
| Terminal agent final answer (`d7748fd1`) | `cluaiz agent --local --max-steps 2`, 6-file sum task, 3 runs | 3 / 3 `Stopped after 2 steps`; result empty (2) or a mid-task sentence (1) | 3 / 3 `Step or repeat limit reached: one last answer ...` notice and a final answer (one says sum 67; the right sum was 153: the 0.8B model read wrong paths) |
| | API `max_steps: 2`, 5 plain + 5 streaming | 5 / 5 + 5 / 5 final answers | 5 / 5 + 5 / 5, answer text identical 10 / 10 |
| | `cargo test -p cluaiz-agents --lib turn::` | - | 13 passed (2 new) |
| response_format with callable tools (`e72069a0`) | run 68 f2: schema + `get_weather`, weather question, x10 | 0 / 10 calls (schema JSON 10 / 10) | 8 / 10 `get_weather` calls (2 empty content) |
| | c_json_schema / b_json_object / j schema with Engine functions, x10 | 10 / 10 schema JSON | 10 / 10 (constrained log 10 / 10) |
| | f schema + client tool on the person prompt, x10 | 10 / 10 valid JSON | 0 / 10 (no constraint by decision; the model called or wrote JSON for the Engine's `math` / `read_item`) |
| | run 69 k3 / c4 schema with thinking and streaming, no client tool, x5 | 5 / 5 | 5 / 5 (reasoning 5 / 5) |
| | run 69 schema + client tool cases, x5 | 5 / 5 | 3 / 5, 0 / 5, 3 / 5 valid JSON (no constraint) |
| | run 68 d / e / g / g2 / i / i2, run 69 r6 | as before | unchanged |
| Image tool steps drop tool_choice (`afb5b8e3`) | red image + math, `tool_choice: required`, x3 | `tool_choice required` strict grammar on all 4 generations and the final answer | only on the first; later steps lazy |
| tool_choice none (`bbdd9706`) | `get_weather` + `none`, x10 plain and x10 streaming | 10 / 10 client calls in both | 0 / 10 calls; the `<tool_call>` text in `content` 10 / 10 in both |
| Image final answer without tools (`99a3000a`) | red image + math, `max_steps: 1`, x3 | final generation renders tools and parses `load_tool` 3 / 3 | no tool format line, no call 3 / 3; 2 / 3 answer with the result |
| Unusable schema keeps the parser (`909a2d2f`) | `$ref` to nothing, thinking on, `tool_choice: none`, x3 | no answer parser 3 / 3 | warning + parser (`Format peg-native`) 3 / 3 |

## Found on the way (not fixed)

- Image requests: after `load_tool`, media steps do not get the loaded function (`ToolTurns::continuation` adds
  `loaded_functions` only on the text path), so the model called `load_tool` again until the repeat limit (3 / 3 in
  both media cases).
- A first version of the tool_choice none fix returned empty `content` with the call in `reasoning_content`
  (`separate_reasoning` takes a leading `<tag>...</tag>` as thinking); the committed version keeps it as content.

## What this does not prove

- Other models and templates, providers (the provider part of `99a3000a` is not tested), audio requests.
- The cut-call rule on the Engine-tool path was not reproduced live (run 70's case did not recur).
- Final answer quality.

## Files

`raw/<set>/` per check (results.jsonl, request_logs.jsonl or the probe's log lines, engine.log, summary.jsonl);
`raw/raw_*.out` console; `raw/mtp_log/`; `raw/cli_<binary>/run_N.json` (headless result with events);
`raw/rf_run68_tables.md`; `raw/agents_turn_tests.txt`; `raw/binaries/`. User name replaced by `<user>`; no token in
any file.

## Pending

| Item | Needs |
|---|---|
| Loaded functions on image / audio tool steps | add `loaded_functions` to the media continuation |
| Provider final answer on media | a provider run with an image |
| Cut client call is not returned in `tool_calls` (llama-server returns it with `finish_reason: length`) | a way to tell client from Engine calls in `native/stream.rs`, if wanted |
| Docs and READMEs | the list in the agent's report (no doc edits in this work by instruction) |
