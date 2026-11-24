# Run 65: are `response_format`, `grammar` and tool calls enforced on a local GGUF model? (2026-11-24)

| | |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 4 GB, shared with other work |
| Server | copy of `target\debug\cluaiz.exe` + DLLs in a scratch folder outside git, isolated `CLUAIZ_HOME`, port 8013 |
| Binary | debug build of `9dfd4f46` (built 14:12, after the last code commit), SHA-256 `5FAC33A30CD2EC0DA7D4B3C64F2DF6165BABE507127D3B2F3E1A12BBB1ECF900`, no new build |
| Model | `qwen3-0.6b-gguf-Q4_K_M` (Qwen3-0.6B-Q4_K_M.gguf), context 16384 |
| Settings | `/v1/chat/completions`, `temperature` 0.8 (log: `Temp(0.8)`, new seed per request), `max_tokens` 200, `think_mode` off, not streamed |
| Script | `probe.py`; raw requests, responses, per-request results and the engine log in `raw/` |

The question came from two Reddit comments: does the Engine hold a small local model to valid JSON, a JSON schema,
a GBNF grammar and valid tool-call syntax, or does it only parse the output afterwards?

## Code path (local llama.cpp model)

1. Request fields: `response_format`, `grammar` (`api/src/handlers/chat/types.rs:93`, `:95`).
2. Copied into the envelope: `api/src/handlers/chat/preflight.rs:1255-1256`.
3. Read in the generation thread: `engine/llama/src/native/stream.rs:137-140` calls
   `OutputConstraint::from_request` (`engine/core/src/chat.rs:92-115`): `grammar` wins; `json_object` becomes the
   schema `{"type": "object"}` (or its `schema`); `json_schema` takes `json_schema.schema`; `text` and other types
   give no constraint. This matches llama-server's `response_format` handling
   (`engine/llama/llama.cpp/tools/server/server-common.cpp:1449-1472`).
4. JSON schema to GBNF: `stream.rs:843` -> `engine/llama/src/ffi/chat.rs:121` -> `engine/llama/src/ffi/chat_bridge.cpp:220-223`,
   which calls llama.cpp's own `json_schema_to_grammar`.
5. Grammar sampler: `stream.rs:849-850` -> `chat_bridge.cpp:298-341`, `llama_sampler_init_grammar` (strict, not
   lazy). It is kept out of the chain and checked per picked token, then over the whole vocabulary on a reject, as
   llama.cpp's `common_sampler_sample` does (`engine/llama/src/native/sampler.rs:22-61`; used at `stream.rs:1017`,
   `:1047`, `:1373`). With thinking on it wakes after the thinking block (`stream.rs:945-954`, `:1116-1118`).
6. Tools: client functions go to the model's own chat template through llama.cpp `common_chat_templates_apply`;
   the returned grammar becomes a lazy grammar with triggers (`chat_bridge.cpp:334-335`,
   `llama_sampler_init_grammar_lazy_patterns`) placed first in the sampler chain (`sampler.rs:84-86`), and
   llama.cpp's parser reads the calls back (`stream.rs:826-835`). So tool calls are grammar-constrained once the
   model starts a call, and also parsed.

Two gaps found in the code:

- **A request grammar or schema is dropped when any tool is in the prompt.** `stream.rs:840`: the request's
  constraint is built only `if tool_grammar.is_null()`. When functions are offered (client tools, or the Engine's
  own `load_tool` / `find_tools` functions, which preflight adds whenever tools are installed, see
  `offer_meta_tools` at `preflight.rs:813-817` and `:944-946`), the tool grammar exists and `response_format` /
  `grammar` are ignored with no warning. llama-server instead puts the schema into the same template call
  (`inputs.json_schema`, `server-common.cpp:1513`; `common/chat-auto-parser-generator.cpp:72-78`: grammar included
  and not lazy when a response format is set) and refuses `grammar` with tools ("Cannot use custom grammar
  constraints with tools.", `server-common.cpp:1543-1546`). The bridge never passes `json_schema`
  (`chat_bridge.cpp:44-60`).
- **`tool_choice` is not passed to llama.cpp.** `chat_bridge.cpp:50` sets `COMMON_CHAT_TOOL_CHOICE_AUTO` for every
  request. The API reads `tool_choice` only for `"none"` (`preflight.rs:692`). `"required"` or a named function is
  not forced. llama-server parses it (`server-common.cpp:1425`, `:1512`) and with `REQUIRED` the call grammar is
  strict, not lazy (`chat-auto-parser-generator.cpp:73-78`).

## Results (10 requests per case)

| Case | Request | Valid | Median time | Median output tokens |
|---|---|---:|---:|---:|
| a | plain prompt, no constraint (strict `json.loads` of the whole answer) | 0 / 10 | 0.206 s | 23 |
| b | `response_format: json_object` | 10 / 10 | 0.320 s | 19 |
| c | `response_format: json_schema` (name string, age integer, both required, no extra keys); parses and matches | 10 / 10 | 0.351 s | 17 |
| d | `grammar: root ::= "yes" \| "no"`; answer exactly `yes` or `no` | 10 / 10 | 0.122 s | 1 |
| e | `get_weather` tool, `tool_choice: "required"`, "What is the weather in Paris right now?"; valid call with JSON `city` | 10 / 10 | 1.955 s | 0 (reported) |
| b/c/d + `tool_choice: "none"` | same three cases | 10 / 10 each | 0.329 / 0.385 / 0.153 s | 18 / 17 / 1 |
| f | case c plus the `get_weather` tool (auto) | 9 / 10 | 0.178 s | 17 |
| g | case d plus the `get_weather` tool (auto) | 0 / 10 | 0.077 s | 2 |
| h | `get_weather`, `tool_choice: "required"`, "Say hello to me in one short sentence." | 0 / 10 tool calls | 0.114 s | 9 |

Notes on each:

- a: all 10 answers were JSON inside a Markdown fence (```` ```json ... ``` ````). With the fence stripped all 10
  parse; as a raw answer none does. This is the failure a constraint removes.
- b, c, d: the log shows `Answer constrained by a JSON schema grammar` / `GBNF grammar` and
  `grammar on (lazy false, 0 triggers)` for every request. No fence, no extra text.
- e: the log shows `grammar on (lazy true, 1 triggers)`: the call grammar is lazy (auto), not forced. All 10
  calls were `get_weather {"city":"Paris"}` because the prompt asks for it. `usage.completion_tokens` is 0 on
  these replies (looks like a reporting gap, not checked further).
- f, g: no `Answer constrained` line in the log; only the lazy tool grammar was on. g answered `Yes.` / `No.`
  (10 / 10 outside the grammar), which proves the grammar was dropped. f's 9 "valid" answers are the model writing
  JSON on its own (one reply was a tool call with empty content); they are not enforcement.
- h: no call in 10 requests; `finish_reason: stop`. `"required"` is not enforced.

Speed: on these short answers a constrained answer took about 0.32-0.39 s against 0.21 s unconstrained, about
17-23 ms per output token against 9 (prompt processing included, debug build, shared laptop). Treat it as a rough
sign, not a measurement.

Answers repeat in runs (for example "Alice", 25 five times in a row) although each request has a new seed. At
0.6B with Top-K 40, Min-P 0.05 the model may simply be very sure; not checked (?).

## Smallest fix (not implemented; for the owner to decide)

1. Constraint with tools: in `chat_bridge.cpp` `inputs_from` (`:44-60`) read `json_schema` from the request body
   into `inputs.json_schema`, and in `stream.rs` (around `:297-360`, where the native apply request is built)
   send the request's schema (from `OutputConstraint::JsonSchema`) in that body. llama.cpp then returns one grammar
   that covers the answer and the calls (`chat-auto-parser-generator.cpp:72-78`). For a raw `grammar` with tools,
   return a 400 like llama-server (`server-common.cpp:1543-1546`) instead of dropping it silently; the Engine's
   own functions would need a decision there (drop them for that request, as `tool_choice: "none"` does).
2. `tool_choice`: pass the request's value through the envelope to the bridge and set
   `inputs.tool_choice = common_chat_tool_choice_parse_oaicompat(...)` at `chat_bridge.cpp:50` (the function
   llama-server uses, `common/chat.cpp:331`). A named function needs the tool list cut to that one function.

## What this run does not prove

- One small model (Qwen3 0.6B), one chat format (`peg-native`), one machine, debug build, thinking off. The
  grammar's wake-up after a thinking block (`stream.rs:1116-1118`) was not exercised.
- The test home had no installed tools, so the Engine's own functions were not in the prompt. The "constraint
  dropped when tools are present" result comes from cases f and g with a client function, which take the same
  branch (`stream.rs:840`); a home with tools installed was not run.
- Streaming (`stream: true`), CEP, Ollama `format`, and cloud providers (`api/src/providers/request.rs:208`
  forwards `response_format` to the provider) were not tested.
- The isolated home's `model_registry.json` was copied from an earlier scratch home and points at that home's
  GGUF file (read only, same file). Nothing in the owner's `~/.cluaiz` or port 8000 was used.
- No unit tests were run.
