# Run 79: image request, the math call after `load_tool` (live check of 46b2ff1e, 2026-11-26)

## What

Run 73 found that on an image request the model called `load_tool` for the math tool and then `load_tool`
again until the repeat limit (3 / 3 in both media cases): the media tool steps were built from the request's
own tools only, so the loaded math functions never reached the model. Fix `46b2ff1e` ("give image and audio
tool steps the functions of tools loaded with load_tool") adds the loaded functions to the media steps. This
run checks live that, after `load_tool`, the next step calls the math function.

## Setup

| Item | Value |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 Laptop 4 GB, shared with another agent's build (free RAM 2.2-6.4 GB during the run, free VRAM 2.2-2.3 GB) |
| Binary | `target/debug/cluaiz.exe`, `cargo build -p cmd` debug of `62aa7c52` (contains `46b2ff1e`), built 2026-11-26 00:12, SHA-256 `e0ac1db1...cfa38cd`; not rebuilt for this run |
| Before | run 73's sets `before_media_*` (binary `7498b6b4`) and `f2after_media_*` (binary `99a3000a`, the last run 73 build for these cases, without `46b2ff1e`); read from `../2026-11-25-run73-tool-loop-and-schema-fixes/raw/` |
| Home | new isolated home `run79home`: `config/` and `tools/` copied from run 73's home (math plugin, filesystem MCP), the three tool paths in `tools_registry.json` rewritten to the new home, `api_port` 8013 in that copy only; no session token copied (the server made its own; it is in no file here) |
| Server | `cluaiz serve` started by the probe, `CLUAIZ_HOME=<home>`, `CLUAIZ_PORT=8013`, `CLUAIZ_HOST=127.0.0.1`, one server per set, stopped by the probe; auth settings untouched |
| Model | `qwen3.5-2b-mtp-gguf-Q4_K_M` (Qwen3.5-2B Q4_K_M + `mmproj-F16.gguf`), the same files run 73 used, full GPU |
| Request | run 64's red PNG + "What is 23 * 19 / 47 * 12 / 315 / 7? Use the math tool.", temperature 0, `think_mode` off, `max_tokens` 256, not streaming |
| Cases | `required`: `tool_choice: "required"`, default `max_steps`; `final`: `max_steps: 1`, no `tool_choice`; n = 3 each |

## Results

Calls per request are read from the engine log lines of that request (`count_media_calls.py`): "model calls"
is one `[NativeTools] Parsed` line per generation, "run" is an `[API] Tool call:` line (a repeated call
the Engine refuses at the repeat limit is parsed but not run).

| Set | Request | Model calls per generation | load_tool run | math run | Tool steps | Limit hit | Answer |
|---|---|---|---|---|---|---|---|
| run 73 before, `required` | 0 / 1 / 2 | load_tool x5 | 2 | 0 | 3 | yes | "I will use the `math_evaluate` function ..." 3 / 3 |
| run 73 f2after, `required` | 0 / 1 / 2 | load_tool x4 | 2 | 0 | 3 | yes | 437, 564, "To calculate ..." (final no-tools turn) |
| **run 79, `required`** | 0 | load_tool / math_evaluate / none | 1 | 1 | 2 | no | "The result of the calculation 23 * 19 is 437." |
| | 1 | load_tool / math_evaluate / none | 1 | 1 | 2 | no | "The result of the calculation 47 * 12 is 564." |
| | 2 | load_tool / math_evaluate / none | 1 | 1 | 2 | no | "The result of 315 / 7 is 45.0." |
| run 79, `required`, `small_model_tools: "full"` | 0 / 1 / 2 | load_tool / math_evaluate / none | 1 | 1 | 2 | no | 437, 564, 45.0 (math plugin results) |
| run 73 before, `final` | 0 / 1 / 2 | load_tool x2 | 1 | 0 | 0 | yes (step) | empty, empty, "To evaluate ..." |
| run 73 f2after, `final` | 0 / 1 / 2 | load_tool | 1 | 0 | 0 | yes (step) | 437, 564, "To calculate ..." |
| run 79, `final` | 0 / 1 / 2 | load_tool | 1 | 0 | 0 | yes (step) | 437, 564, "To calculate $315 / 7$, I will use ..." |

- `required`: the math call after `load_tool` 0 / 3 -> 3 / 3; repeat limit 3 / 3 -> 0 / 3; tool steps 3 -> 2;
  the math plugin returned the result and the model answered with it 3 / 3 (wall time 19-24 s per request).
  Log for request 0: `Parsed 1 call(s): ["load_tool"]` -> `load_tool("math") -> Some("math")` -> Step 1 ->
  `Parsed 1 call(s): ["math_evaluate"]` -> `Tool call: plugin 'math' (sub_func: Some("math_evaluate")) with
  payload: {"expression":"23 * 19"}` -> Step 2 -> `Parsed 0 call(s)` and the answer.
- `final` (`max_steps: 1`): unchanged from run 73's f2after: the one allowed step is `load_tool`, the turn stops
  at the step limit and the final no-tools turn answers. This case cannot show the math call (only one step is
  allowed); it shows the fix did not add tools to the final answer (no call parsed in it, 3 / 3). The 437 and
  564 here are the model's own arithmetic, not the plugin's.

## Differences from run 73's conditions

- Tool set: run 79's binary has the small-model rule (run 77): the 2B file has 1.94 billion weights, so the
  Engine sent only client tools, bound tools and tool search and left out `read_item` (log `Small model
  (1942653248 weights, under 2000000000)` per request). Run 73 sent the full set.
  To match run 73, the `required` case was run again with `context_policy.small_model_tools: "full"`
  (`raw/after_media_required_full/`, no `Small model` line): the same result, load_tool -> math_evaluate ->
  answer 3 / 3, 2 steps, no limit (wall time 24-30 s).
- Context: the planner picked 11,008 tokens (`required`), 8,192 (`required` full) and 4,096 tokens KV Kv4 (`final`) for the free VRAM at
  start; run 73 had 16,384 tokens. The prompts are far shorter than either.

## Failed attempts (kept, not counted)

- `raw/attempt1_ram500_required/`: 3 / 3 HTTP 500 `Media needs 1742573443 host bytes; live RAM budget is
  996601446` (free RAM 3.8 GB, the other agent's build); the same set passed minutes later.
- `raw/attempt2_ram500_required_full/`, `raw/attempt3_ram500_required_full/`: `small_model_tools: "full"`,
  3 / 3 HTTP 500 each for the same reason (`live RAM budget`, once `Insufficient live RAM for media
  request`); the set passed when free RAM was 6.4 GB.

## What this proves and what it does not

Proves, on this model and prompt: after `load_tool` on an image request, the next step gets the math function
and calls it (3 / 3 with the default lean tool set, 3 / 3 with the full set), the turn ends without the repeat limit, and the `max_steps: 1` final answer still gets no
tools.

Does not prove: audio requests (same code path in `46b2ff1e`, not run); provider models (the provider start
prompt part of the fix); other models or templates; streaming; more than 3 requests per case.

## Files

| Path | What |
|---|---|
| `media_tools_probe.py` | run 73's probe; this copy reads the model from `PROBE_MODEL` and adds `--small-model-tools` |
| `count_media_calls.py` | per-request call table from a set's `results.jsonl` + `request_logs.jsonl` |
| `raw/after_media_required/`, `raw/after_media_required_full/`, `raw/after_media_final/` | `results.jsonl`, `request_logs.jsonl`, `engine.log` |
| `raw/attempt1_ram500_required/`, `raw/attempt2_ram500_required_full/`, `raw/attempt3_ram500_required_full/` | the failed sets |
| `raw/tables.md` | `count_media_calls.py` output for run 73's four sets and run 79's sets |

User name replaced by `<user>`; no token or key in any file.

Rerun (needs run 70's `loop_probe.py` and run 64's `vision_hint.py`):

```
PROBE_RUNS=<repo>/test/cloud/runs PYTHONPATH=<repo>/test/cloud/runs/2026-11-24-run70-tool-loop-final-answer \
python -B media_tools_probe.py --exe <repo>/target/debug/cluaiz.exe --home <home> --port 8013 --out <dir> --case required --n 3 [--small-model-tools full]
python -B count_media_calls.py <dir>
```

## Pending

| Item | Needs |
|---|---|
| Audio request with a loaded tool | an audio model and a tool prompt |
| Provider model with an image and `load_tool` | a provider key and run |
