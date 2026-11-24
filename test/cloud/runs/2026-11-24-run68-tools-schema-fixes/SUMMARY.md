# Run 68: tools on image requests, and response_format / grammar / tool_choice with tools (2026-11-24)

Before and after proof for two fixes found in run 64 (an image request got no tool schemas) and run 65 (a request's
grammar or JSON schema was dropped whenever a tool was in the prompt; `tool_choice` was always `auto`).

| | |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 4 GB, shared with other work |
| Server | a copy of `target\debug\cluaiz.exe` + DLLs in a scratch folder outside git, isolated `CLUAIZ_HOME` (new, only a `model_registry.json` pointing at GGUF files of earlier scratch homes, read only), port 8013, auth settings at their defaults |
| Binaries | debug builds (`cargo build -p cmd`): before = `961a673f` (SHA-256 `97d985360b685183cec75bc018173cdef9a74d2bc05c07550f186951a616dde4`); after bug 1 = `790819e2` (`1860bd6aaa159edd2cd8f2f735222052826806c7cdf833fdd38589d96e10e809`); after bug 2 = the code of `5cec4478` except one warning condition, full run; final = `5cec4478` (`f353bdb12ce4d39c671ae8e606a5bd6f1eed8ac23126c18762dc39e890a5efaf`), 3 requests per case |
| Models | vision: `qwen3.5-2b-mtp-gguf-Q4_K_M` (Qwen3.5-2B Q4_K_M + mmproj-F16; the 0.8B of run 64 was not on this laptop), temperature 0, `max_tokens` 128; text: `qwen3-0.6b-gguf-Q4_K_M`, run 65's settings (temperature 0.8, `max_tokens` 200) |
| Thinking | `think_mode: off` everywhere; a strict grammar over a thinking block was not exercised |
| Script | `fixcheck.py` (reuses run 64's `vision_hint.py` image and `get_weather`, run 65's `probe.py` cases and judges); `summarize.py` makes `raw/tables.md` |

`raw/<binary>/`: `results.jsonl` (every request: status, judged result, answer, `prompt_n`, and the engine log lines of
that request), `summary.jsonl`, `engine.log`, `console.txt`. User name replaced by `<user>`.

"Constrained" counts requests whose own log shows `Answer constrained` or `grammar on (lazy false`; lines after the
request's last `Parsed N call(s)` line are not counted, because a chat with a `session_id` starts its background card
job right after the reply and that job sets its own GBNF grammar (`api/src/handlers/chat/background.rs`).

## Bug 1: an image request had no function schemas

Cause (two cut points):
1. `api/src/handlers/chat/preflight.rs`: the envelope struct was built with `tools: None` and the tools were patched
   into the prompt JSON afterwards; the image envelope (`prepared_media`) is cloned from the struct before that patch.
2. `engine/llama/src/native/stream.rs:286` (before the fix): `req_tools.filter(|_| !is_media && !is_pivot)` dropped the
   tools for any prompt with media. From `4dc7016b` (2026-10-24), when an inline tool form still covered media.

Fix (`790819e2`): tools in the struct; media prompts take the native tool path, as llama-server renders tools for a
multimodal request (`oaicompat_chat_params_parse`, `tools/server/server-common.cpp`).

| Case (Qwen3.5-2B, 10 each) | Before | After bug 1 | After bug 2 |
|---|---|---|---|
| image + weather question + `get_weather` | 0 / 10 calls, prompt_n 225 | 10 / 10 get_weather({"city":"Paris"}), prompt_n 482 | 10 / 10 |
| the same without the image | 10 / 10, 284 | 10 / 10 | 10 / 10 |
| image + "What colour is this image? One word.", no tools | 10 / 10 red | 10 / 10 | 10 / 10 |

## Bug 2: response_format, grammar and tool_choice with tools

Cause: `stream.rs` built the request constraint only `if tool_grammar.is_null()` (since `ce1372dc`, 2026-10-24);
`chat_bridge.cpp` never set `inputs.json_schema` and hard-coded `COMMON_CHAT_TOOL_CHOICE_AUTO`; the API read
`tool_choice` only for "none".

Fix (`5cec4478`): schema into `inputs.json_schema` (one strict grammar for answer + calls,
`chat-auto-parser-generator.cpp:72-78`; the strict template grammar is fed the generation prompt and applies from the
first token as `common_sampler` does); grammar + client tools -> HTTP 400 (`server-common.cpp:1543-1546`); grammar +
only Engine functions -> those left out (logged); `tool_choice` -> `inputs.tool_choice`
(`common_chat_tool_choice_parse_oaicompat`); named function -> only that client function, sent as "required"
(llama-server does NOT do this: `server-common.cpp:1425` reads a string, an object falls back to "auto",
`server-common.h:45-56`).

| Case (Qwen3-0.6B, 10 each) | Before: valid / constrained | After: valid / constrained |
|---|---|---|
| b json_object, no tools | 10 / 10 | 10 / 10 |
| c json_schema, no tools | 10 / 10 | 10 / 10 |
| d GBNF yes/no, no tools | 10 / 10 | 10 / 10 |
| e get_weather, required, weather question | 10 / 0 (lazy) | 10 / 10 (strict) |
| f json_schema + get_weather (auto) | 6 / 0 | 10 / 10 |
| f2 json_schema + get_weather, weather question | 10 / 0 (calls) | 10 / 10 (calls) |
| j json_schema + only Engine functions (session_id: remember, forget) | 0 / 0 (called remember, answered text) | 10 / 10 |
| g GBNF + get_weather (after: expect 400) | HTTP 200, `Yes.` outside grammar | HTTP 400 10 / 10 |
| g2 GBNF + only Engine functions | 0 / 0 (`Yes.`) | 10 / 10, log "Engine's own functions are left out" |
| h required, "Say hello to me in one short sentence." | 0 / 10 calls, finish stop | 2 / 10 calls, 10 / 10 strict |
| i named get_time, weather question | 0 / 10 (called get_weather) | 10 / 10 get_time |
| i2 named "nope" (after: expect 400) | HTTP 200 | HTTP 400 "tool_choice names the function 'nope', which is not in tools" |

Case h: llama.cpp's own required grammar allows text before the call (`content_before_tools = p.until(trigger_marker)`,
`chat-auto-parser-generator.cpp:473-474`) and forbids ending without a call; 8 / 10 ran to max_tokens 200
(finish_reason length). llama-server behaves the same; not changed.

Final binary (`5cec4478`, 3 each): image + tool 3/3, c 3/3, e 3/3, f 3/3, g 400 3/3, g2 3/3, j 3/3, i 3/3; no
"not enforced" / "did not take" warning in the log.

Unit test: `cargo test -p cluaiz-core --lib -- chat::tests`: 11 passed. `tool_choice_takes_openai_forms` (api) not run.

## Not proven
- One vision model (2B), one text model / format (peg-native), one machine, debug builds, thinking off; streaming,
  specialized templates that may ignore json_schema (now logged), audio + tools not run.
- The agent loop after `required` made the Engine run one of its own functions (code drops tool_choice for later
  steps; not exercised).
- Providers: tool_choice still not forwarded; a named choice only narrows `tools` there.
- api / llama / runtime / cmd test suites not run.
