# Run 80: JSON schema with tools, the opt-in "tool call or schema JSON" grammar, off and on (2026-11-26)

## Setup

| Item | Value |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 Laptop 4 GB |
| Build | debug `cargo build -p cmd` of `62aa7c52` (includes `3f8e69a4` / `62aa7c52` "opt-in grammar for response_format with tools" and the small-model tool rule), SHA-256 `e0ac1db16b73eb64...` |
| Server | started by run 68's `fixcheck.py` (unchanged, from its own folder): isolated `CLUAIZ_HOME` (the run 79 test home, no session token), `CLUAIZ_PORT=8013`, `CLUAIZ_HOST=127.0.0.1`; auth settings untouched |
| Model | `qwen3-0.6b-gguf-Q4_K_M` (596,049,920 weights, so the small-model rule applied: `read_item` left out, tool search kept) |
| Cases | run 68's `f_json_schema_with_tool` (person prompt, schema + client `get_weather`; pass = valid schema JSON) and `f2_schema_tool_weather_question` (weather question, schema + `get_weather`; pass = a `get_weather` call or valid schema JSON), n = 10 each, temperature 0.8 (run 65's settings) |
| Switch | `CLUAIZ_JSON_TOOLS_GRAMMAR=off` (default) and `=on`, same binary, same home |
| Rerun | from the repository root: `FIXCHECK_RUNS=$PWD/test/cloud/runs PYTHONPATH=<run65>:<run64> CLUAIZ_JSON_TOOLS_GRAMMAR=on python -B test/cloud/runs/2026-11-24-run68-tools-schema-fixes/fixcheck.py --exe target/debug/cluaiz.exe --home <isolated-home> --port 8013 --out <dir> --cases f_json_schema_with_tool,f2_schema_tool_weather_question --n 10` |

## Results

| Case | Switch off | Switch on |
|---|---|---|
| Person prompt, schema + tool: valid schema JSON | 3 / 10 | 8 / 10 |
| Weather question, schema + tool: call or JSON | 10 / 10 (10 `get_weather` calls) | 10 / 10 (10 `get_weather` calls) |
| Log line "the answer is a tool call or the schema's JSON, one grammar" | 0 | 38 (every generation) |

What the failures were (`raw/flag-*/results.jsonl`, field `content` and the `Parsed` log lines):

- Off, person prompt: 7 of 10 replies were not the schema. In 6 the model went through tool search (`find_tools`,
  `load_tool`, `read_tool_file`) into the math plugin or a file tool and returned their output or call-shaped text
  (for example `{"name": "load_tool", "arguments": {"id": "math"}}`); 1 refused.
- On, person prompt: 2 of 10 not valid: one called `get_weather` (a call is allowed by the grammar; the judge wants
  JSON for this prompt) and one went through tool search and returned empty content.

## What this proves

- With the switch on, the answer of a request with `response_format` and tools is constrained to "a tool call or the
  schema's JSON": on the person prompt valid JSON went from 3 / 10 to 8 / 10, and on the weather question the calls
  stayed 10 / 10 (run 68's 0 / 10 calls with a schema-only grammar does not come back).
- Off keeps the 2026-11-25 behaviour (calls work, JSON not constrained).

## What this does not prove

- One small model (Qwen3 0.6B) and one template family; Gemma 4, Mistral, DeepSeek and gpt-oss templates are not
  checked (the code falls back to today's behaviour for pattern triggers and bare-JSON call formats).
- Streaming and thinking-on were not run here.
- n = 10 per case, one pass.
- Tool search is still offered to small models on a prompt that needs no tool; with the switch off the model wandered
  into it on 6 / 10 person prompts. The repeat guard and the small-model rule are separate work.

## Files

`raw/flag-off/` and `raw/flag-on/`: `results.jsonl` (each request with its reply and engine log lines), `summary.jsonl`,
`events.jsonl`, `engine.log`. User name replaced with `<user>`; no token in any file.

## Pending

| Item | Needs |
|---|---|
| Default on | the owner's decision, after a run with more templates |
| Streaming and thinking on | the same cases with `stream: true` and thinking on |
| Other templates | Gemma 4, Mistral, gpt-oss runs |
