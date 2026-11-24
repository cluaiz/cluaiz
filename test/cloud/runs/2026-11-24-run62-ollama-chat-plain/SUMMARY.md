# Run 62: Ollama `/api/chat` sends only the client's messages and tools (2026-11-24)

| | |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 4 GB, shared with other work |
| Server | isolated Cluaiz home (a copy outside any git checkout), port 8013 |
| Model | qwen3.5-0.8b-gguf-Q4_K_M |
| Before | debug binary SHA-256 `53CE960C...` |
| After | `cargo build -p cmd`, `target\debug\deps\cluaiz.exe`, SHA-256 `9A1E9937...` (full hashes in BENCHMARKS row 62) |
| Script | `probe.py` in this folder; raw requests, responses, results and engine logs in `raw/before/` and `raw/after/` |

## Cause

`api/src/handlers/ollama.rs` set `plain_prompt` only for generate (`chat_request.plain_prompt = kind == Kind::Generate;`,
line 117 before the fix). `/api/chat` therefore ran the full chat pipeline (`api/src/handlers/chat/preflight.rs`),
which added the Engine's functions (`load_tool`, `unload_tool`, `read_tool_file`, `find_tools`, `read_item`), the
project's skill index, a workspace note and a `<tools_hint>` block. The before log shows `Conversation 120 tokens
(2 messages)` for a "hi" where the client sent one message.

Ollama's `ChatHandler` (`competitor-repos/ollama/server/routes.go` at b8db8d5: `chatMessagesForRequest` :3655,
`processedTools := req.Tools` :3281, `chatPrompt(..., msgs, processedTools, ...)` :3301) renders only the request's
messages with the request's tools.

## Fix

`ollama.rs` sets `plain_prompt = true` for chat as well as generate. `plain_prompt` already turns off every Engine
tool, notes and project files; the client's own function tools are not gated by it and still reach the template.
The field is `#[serde(skip)]`, so `/v1/chat/completions` and CEP never set it.

## Results

| Request | Before | After |
|---|---:|---:|
| `/api/chat` "hi", no tools | 921 prompt tokens | 13 prompt tokens ("Hello! How can I help you today?") |
| same, streamed | 921 | 13 |
| `/api/chat` with client `get_weather` | `get_weather {"city": "Paris"}`, 987 tokens | same call, 280 tokens |
| `/v1/chat/completions` "hi" | 921 | 921, rendered prompt byte-identical |
| `/v1/chat/completions` with `get_weather` | `get_weather {"city":"Paris"}` | same |

All 5 checks passed in both runs (`raw/*/results.jsonl`).

## What this run does not prove

- The rendered `/api/chat` prompt was not read directly: `return_prompt` does not pass through the Ollama adapter.
  "13 tokens is the template around one user message" is inferred from the count and the `1 messages` log line.
- No unit test for `plain_prompt` on chat (it is set inside `run()`, which needs app state). The api, runtime,
  llama, onnx, cmd and automation-host suites were not run.
- Ollama also adds the model package's own system text and messages (`m.System`, `m.Messages`); the Engine has no
  such field.
- One small model, one machine, debug build.
