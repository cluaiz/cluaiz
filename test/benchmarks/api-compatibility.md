# OpenAI and Ollama API compatibility

## Answer

| Area | Latest result | Run |
|---|---|---|
| Ollama `/api/chat` sends only the client's messages and tools | "hi": 921 -> 13 prompt tokens | 62 |
| Ollama `/api/generate`, long prompt, chat KV size | "The capital of France is": unrelated answer -> "Paris", 4,056 -> 17 prompt tokens | 57 |
| `/api/ps` with the Ollama Python library | empty, then the loaded model with its planned VRAM and context | 37 |
| Official Ollama Python library 0.6.1 | chat, stream, think, format, generate, list, version, embed all work in Ollama's shape | 36 |
| `/api/tags` dates and `keep_alive` | file time as `modified_at`; "5m", "30s", "1h", "-1", 0, 5 accepted; "soon" is HTTP 422 | 35 |
| OpenAI `stop` and `max_completion_tokens` | `stop` (string or list, streaming too) ends before the match; `max_completion_tokens: 5` gives 5 tokens | 34 |
| First API request docs page | answer, stream, 400, 401 after restart | 29 |

Below: every current run of this test type, newest first, in full.

<a id="run-62"></a>

## Run 62 (2026-11-24)

| | |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 4 GB, shared with other work |
| Server | isolated Cluaiz home (a copy outside any git checkout), port 8013 |
| Model | qwen3.5-0.8b-gguf-Q4_K_M |
| Before | debug binary SHA-256 `53CE960C...` |
| After | `cargo build -p cmd`, `target\debug\deps\cluaiz.exe`, SHA-256 `9A1E9937...` (full hashes in BENCHMARKS row 62) |
| Script | `probe.py` in this folder; raw requests, responses, results and engine logs in `raw/before/` and `raw/after/` |

#### Cause

`api/src/handlers/ollama.rs` set `plain_prompt` only for generate (`chat_request.plain_prompt = kind == Kind::Generate;`,
line 117 before the fix). `/api/chat` therefore ran the full chat pipeline (`api/src/handlers/chat/preflight.rs`),
which added the Engine's functions (`load_tool`, `unload_tool`, `read_tool_file`, `find_tools`, `read_item`), the
project's skill index, a workspace note and a `<tools_hint>` block. The before log shows `Conversation 120 tokens
(2 messages)` for a "hi" where the client sent one message.

Ollama's `ChatHandler` (`competitor-repos/ollama/server/routes.go` at b8db8d5: `chatMessagesForRequest` :3655,
`processedTools := req.Tools` :3281, `chatPrompt(..., msgs, processedTools, ...)` :3301) renders only the request's
messages with the request's tools.

#### Fix

`ollama.rs` sets `plain_prompt = true` for chat as well as generate. `plain_prompt` already turns off every Engine
tool, notes and project files; the client's own function tools are not gated by it and still reach the template.
The field is `#[serde(skip)]`, so `/v1/chat/completions` and CEP never set it.

#### Results

| Request | Before | After |
|---|---:|---:|
| `/api/chat` "hi", no tools | 921 prompt tokens | 13 prompt tokens ("Hello! How can I help you today?") |
| same, streamed | 921 | 13 |
| `/api/chat` with client `get_weather` | `get_weather {"city": "Paris"}`, 987 tokens | same call, 280 tokens |
| `/v1/chat/completions` "hi" | 921 | 921, rendered prompt byte-identical |
| `/v1/chat/completions` with `get_weather` | `get_weather {"city":"Paris"}` | same |

All 5 checks passed in both runs (`raw/*/results.jsonl`).

#### What this run does not prove

- The rendered `/api/chat` prompt was not read directly: `return_prompt` does not pass through the Ollama adapter.
  "13 tokens is the template around one user message" is inferred from the count and the `1 messages` log line.
- No unit test for `plain_prompt` on chat (it is set inside `run()`, which needs app state). The api, runtime,
  llama, onnx, cmd and automation-host suites were not run.
- Ollama also adds the model package's own system text and messages (`m.System`, `m.Messages`); the Engine has no
  such field.
- One small model, one machine, debug build.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-24-run62-ollama-chat-plain/`](../cloud/runs/2026-11-24-run62-ollama-chat-plain/)

<a id="run-57"></a>

## Run 57 (2026-11-24)

Windows 11 laptop (Ryzen 7 7435HS, RTX 3050 4 GB), shared with other agents. Model
`qwen3.5-0.8b-gguf-Q4_K_M`. Isolated homes copied from `output/release-soak-home` (run 53), models
hard-linked, registry paths pointed at each copy. Port 8014. Neither port 8000 / 8011 nor `~/.cluaiz`
was used.

- Before: existing `target\debug\cluaiz.exe`, SHA-256 `9A91A8A73339F3F6...` (the run 55 / 56 binary).
- After: `cargo build -p cmd` in `target`, `target\debug\deps\cluaiz.exe`, SHA-256
  `0C9172C5601C39740431CE277A812E8F99882B2030006094E81EAC03505A25FC`. That tree also held other
  agents' uncommitted work (llama.cpp submodule, `engine/plan`, `engine/runtime/src/native_load.rs`);
  none of it is in the request paths checked here.

Two homes with the same contents:

- **in repo**: `<workspace>/output/run-ollama-gen-home`, inside the workspace git checkout, as the run
  52 and 53 homes were;
- **outside**: `%LOCALAPPDATA%\cluaiz-test\run-ollama-gen-home`, as the run 49 home was.

`probe.py` runs five checks against each (`python -B probe.py --exe <exe> --home <home> --port 8014
--label <label>`): the run 53 `ollama_generate` request unchanged, the same streamed, a plain
`/v1/chat/completions` with `return_prompt` (to read `context_report.plan.project`), the run 52
`long_input` request unchanged, and the run 49 `chat_tools` request (a client `get_weather` function).
Both homes were deleted after the run.

#### Results

| Check | before, in repo | before, outside | after, in repo | after, outside |
|---|---|---|---|---|
| `/api/generate` "The capital of France is" | "**Cluaiz** is the AI engine where models, voice, vision,", 4,056 prompt tokens | Paris, 881 | Paris, 17 | Paris, 17 |
| same, streamed | same as above, 4,056 | Paris, 881 | Paris, 17 | Paris, 17 |
| `/v1/chat/completions`, same text: project root, prompt tokens | the workspace checkout, 13,193 chars of agent files, 4,056 | the home's `workspace`, 546 chars, 881 | the home's `workspace`, 546 chars, 883 | the home's `workspace`, 546 chars, 881 |
| `long_input` (1,500 words, think off, 48 tokens) | pass ("The marker is **4917**. This is the **Cluaiz** project identifier...") | fail: content "", `reasoning_content` = `{"arguments":{"path":"."},"name":"read_text_file"}` | fail: content "\n", no `reasoning_content`, 4 tool events | fail: content "\n", no `reasoning_content`, 5 tool events |
| `chat_tools` (client function) | `get_weather {"city":"Paris"}`, `tool_calls` | same | same | same |

Raw: `raw/<label>/` (`results.jsonl`, `requests.jsonl`, `responses.jsonl`, `engine.log`); every file is
under 1 MB. Fix B unit tests: `raw/agents_workspace_tests.txt` (`cargo test -p cluaiz-agents --lib
workspace::`, 15 passed).

#### What was wrong

**Bug 1, `/api/generate` answered about Cluaiz.** The big prompt came from where the test home was,
not from the request. The run 52 and 53 homes were under `output/` of the workspace, which is a git
checkout. The default workspace is `<CLUAIZ_HOME>/workspace` (`api/src/state.rs`), and the agent-file
scan climbed from there to the nearest `.git` (`project_folders`, `agents/src/workspace/mod.rs`), so
the workspace's `AGENTS.md` and `.agents/` (13,193 characters, about 3,700 tokens) went into the
system prompt of every chat (`preflight.rs`, `project::for_chat`), with `read_item` offered for the
rest. `/api/generate` runs the chat pipeline, so it got all of that too, and the 0.8B model described
Cluaiz. The same binary with the home outside the checkout answered Paris with 881 prompt tokens,
the run 49 number.

**What a normal user gets.** With the usual home `~/.cluaiz`, which is not inside a git repository,
the agent-file part never happened: before any fix, such a user got the "outside" column (Paris, 881
prompt tokens on `/api/generate`, most of them the Engine's tool list). After the fixes,
`/api/generate` sends only the prompt in the model's template (17 tokens), and `/v1/chat/completions`
is unchanged for that user (881).

Ollama's own generate builds the system text and the prompt and renders them with an empty tool
list (`competitor-repos/ollama/server/routes.go`, `GenerateHandler`: `[]api.Tool{}`); its
`GenerateRequest` has no `tools` field (`api/types.go`).

**Bug 2, empty answer with a tool call as reasoning.** Two parts. (1) With any tool installed, a
chat's first message is offered the tool list (by design, `preflight.rs`); on the 1,500-word prompt
the 0.8B model calls the same tool again and again, and the turn stops after four identical calls
(`agents/src/turn.rs`, `REPEAT_STOP_THRESHOLD`). (2) The non-streaming answer then took the last
generation whole, `<tool_call>{...}</tool_call>`, and `separate_reasoning`'s generic fallback treats
any leading `<tag>...</tag>` as thinking, so the call came back as `reasoning_content` with an empty
answer. Part (2) is fixed; part (1) is the model's behaviour with tools offered and still makes the
check fail (content is now only a newline).

**Bug 3, "Saved chat run49-cache (92.8 MB)".** Intended. That chat held 4,667 positions (most of them
the agent files of Bug 1). The `.kv` (77,680,876 bytes) is llama.cpp's sequence state: F16 attention
KV of 6 attention layers x 2 KV heads x 256, 12,288 bytes per position (57.3 MB), plus the
linear-attention state; the `.ckpt` (20,201,936 bytes) is the newest checkpoint, kept so a hybrid
model can restart before its last answer. Explained in `engine/runtime/README.md`, commit `0f320e5a`.

#### Fixes

| Fix | Change | Proof |
|---|---|---|
| A: `/api/generate` is a plain prompt | `ExternalChatRequest.plain_prompt` (`#[serde(skip)]`, set only by `ollama.rs` for generate): no notes, no project agent files, no tools; generate's `tools` field is not passed on | 17 prompt tokens and Paris in both homes; `/v1/chat/completions` tools still work |
| B: the default workspace is its own root | `agent_folders` in `agents/src/workspace/mod.rs` (uses `EnvironmentManager::is_unchosen_workspace`); a folder the user opens inside the same checkout still reads it | in-repo `/v1` request 4,056 -> 883 tokens, project root the home's `workspace`; unit test |
| Bug 2: no call as reasoning | `non_streaming.rs`: at `Ended::StepLimit` only the text before `<tool_call>` is the answer | `reasoning_content` gone in both homes |

#### Not done

| Item | Why it matters | Needs |
|---|---|---|
| `long_input` still fails on the 0.8B model: it calls tools until the repeat limit and says nothing | an empty answer to a long plain prompt | a decision on offering tools for messages that do not ask for any, or a final answer step after the repeat limit |
| Streaming path at the step limit not checked | the same leak may exist there | a streamed `long_input` |
| `/api/chat` still gets the Engine's additions | Ollama chat clients see Engine functions | owner decision (see `api/src/handlers/README.md`, Pending) |
| api and runtime test suites not run | owner rule for this task | CI with the CPU backend |

Logs, raw results and scripts: [`test/cloud/runs/2026-11-24-run57-ollama-generate/`](../cloud/runs/2026-11-24-run57-ollama-generate/)

<a id="run-37"></a>

## Run 37 (2026-11-21)

Windows 11 (build 26300), Engine 0.1.0 debug build, Qwen3-0.6B Q4_K_M, isolated `CLUAIZ_HOME` on port
8011, `ollama` 0.6.1. Script: `probe.py`; output: `results.txt`.

| Call | Result |
|---|---|
| `client.ps()` before any chat | `models=[]` |
| `client.ps()` after one chat | one model: `qwen3-0.6b-gguf-Q4_K_M`, `size` 396,705,472 bytes (its file), `size_vram` 1,597,270,011 (the VRAM its plan was given, KV cache and buffers included), `context_length` 16384, `format` gguf, `family` qwen3 |

Code: `api/src/handlers/ollama.rs` (`ps`), from the host's current plan and the model registry.

#### Not proven

`size_vram` is the planned budget, not a driver reading. `parameter_size` is `Unknown` for this model
(the registry has no count). `expires_at` and `digest` are not sent.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-21-run37-ollama-ps/`](../cloud/runs/2026-11-21-run37-ollama-ps/)

<a id="run-36"></a>

## Run 36 (2026-11-21)

Windows 11 (build 26300), Engine 0.1.0 debug build (`cargo build -p cmd`), Qwen3-0.6B Q4_K_M, isolated
`CLUAIZ_HOME` on port 8011, `ollama` Python library 0.6.1. Script: `probe.py`; output: `results.txt`.

| Call | Run 33 (before, raw HTTP) | Run 36 (after, the library) |
|---|---|---|
| `client.chat(...)` | `/api/chat` answered in OpenAI's shape | `content='hello'`, `done=True`, `done_reason=stop`, `eval_count=1`, `prompt_eval_count=19` |
| `client.chat(..., stream=True)` | SSE in OpenAI's shape | 26 lines of JSON, counting 1 to 5, final `done=True`, `eval_count=25` |
| `think=True` | `think` ignored | 765 characters in `message.thinking`, answer `2 + 3 = 5.` |
| `format="json"` | - | `{"city": "Paris"}`, parses as JSON |
| `client.generate(..., system=...)` | 404 | `response='yes'`, `done=True` |
| `client.list()` | worked | `['qwen3-0.6b-gguf-Q4_K_M']` |
| `GET /api/version` | 404 | `{"version":"0.1.0"}` |
| `client.embed` with a chat model | 404 | HTTP 400 `Selected model does not declare an embedding task`: the same refusal as `/v1/embeddings` |

#### What changed (code)

`api/src/handlers/ollama.rs`: `/api/chat`, `/api/generate`, `/api/embed`, `/api/version`. An Ollama request
becomes the OpenAI request of the chat or embeddings handler (`options`, `think`, `format`, `keep_alive`
in seconds, `images`, `tools`), and the answer goes back in Ollama's shape: one object, or one JSON object
per line when streaming. Unit tests in the file (request, `keep_alive`, generate, final object).

#### Not proven

One model, one platform. `num_ctx`, `images`, `tools` and `/api/embed` with an embedding model were not
sent. The Ollama JavaScript library was not run. The `api` test suite (with the new unit tests) was not
run: it rebuilds llama.cpp with CUDA; the library run is the check. `/api/ps` and `/api/show` are still 404.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-21-run36-ollama-library/`](../cloud/runs/2026-11-21-run36-ollama-library/)

<a id="run-35"></a>

## Run 35 (2026-11-21)

Windows 11 (build 26300), Engine 0.1.0 debug build, Qwen3-0.6B Q4_K_M, isolated `CLUAIZ_HOME` on port
8011. Script: `probe.py`; output: `results.txt`; the unload times are the server's own log lines.

| Request | Run 33 (before) | Run 35 (after) |
|---|---|---|
| `GET /api/tags` | `modified_at` always `2026-10-08T00:00:00Z` | `2026-11-21T01:53:27Z`, the model file's change time |
| `keep_alive: "5m"` | HTTP 422 | HTTP 200; log: unload in 5 minutes |
| `keep_alive: "30s"` | - | HTTP 200; log: unload in 1 minute (seconds round up) |
| `keep_alive: "1h"` | - | HTTP 200; log: unload in 60 minutes |
| `keep_alive: "-1"`, `0`, `5` | - | HTTP 200; no unload, unload after the answer, unload in 5 minutes |
| `keep_alive: "soon"` | - | HTTP 422 |

#### What changed (code)

- `api/src/utils/mod.rs`: `keep_alive_minutes` reads a number (minutes, as before) or a string with `s`,
  `m` or `h`; used by chat, embeddings, speech and transcription requests. Unit test
  `keep_alive_takes_minutes_or_ollama_durations`, run on its own with `rustc --test`: 1 passed.
- `api/src/handlers/models.rs`: `/api/tags` sends the main file's change time.

#### Not proven

`parameter_size` still comes from the registry (`Unknown` for this model). The `api` test suite was not
run (it rebuilds llama.cpp with CUDA); speech and transcription requests with a string `keep_alive`
were not sent.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-21-run35-ollama-tags-keep-alive/`](../cloud/runs/2026-11-21-run35-ollama-tags-keep-alive/)

<a id="run-34"></a>

## Run 34 (2026-11-21)

Windows 11 (build 26300), Engine 0.1.0 debug build (`cargo build -p cmd`), Qwen3-0.6B Q4_K_M, isolated
`CLUAIZ_HOME` on port 8011, `think_mode: "off"`, `temperature: 0`. The same kind of requests as run 32,
after the fix. Script: `probe.py`; its output: `results.txt`.

| Request | Run 32 (before) | Run 34 (after) |
|---|---|---|
| `"stop": "5"` (a string), count 1 to 10 | HTTP 422 | HTTP 200, answer `1..4`, `finish_reason: stop`, 8 tokens |
| `"stop": ["5"]` | answer went on to 10 | answer `1..4`, `finish_reason: stop` |
| `"stop": ["7", "3"]` | - | answer `1, 2`: the first match in the text wins |
| `"stop": ["5"]`, streaming | - | streamed text `1..4`, `finish_reason: stop` |
| `"max_completion_tokens": 5` | 576 tokens | 5 tokens, `finish_reason: length` |
| `max_tokens: 8` and `max_completion_tokens: 3` | - | 8 tokens: `max_tokens` wins |
| no `stop` (control) | - | `1..10`, 20 tokens |

#### What changed (code)

- `api/src/handlers/chat/types.rs`: `stop` is a string or a list (`StopList`); `max_completion_tokens` is
  read, and `answer_limit()` uses it when `max_tokens` is missing.
- `engine/core/src/chat.rs`: `ChatEnvelope.stop` carries the strings to the runtime.
- `engine/llama/src/native/stream.rs`: answer text that could begin a stop string is held back; at a
  match the text before it is sent and generation ends (`stop_match`, unit test `stop_tests`, run on its
  own with `rustc --test`: 1 passed).

#### Not proven

One model, one platform. Media requests (images, audio) with `stop` were not sent. The `api` and `llama`
crate test suites were not run (they rebuild llama.cpp with CUDA); the change was checked by this run.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-21-run34-openai-stop-and-limit/`](../cloud/runs/2026-11-21-run34-openai-stop-and-limit/)

<a id="run-29"></a>

## Run 29 (2026-11-21)

Windows 11, Engine 0.1.0 (`target/release/cluaiz.exe`), an isolated `CLUAIZ_HOME` on port 8011 (never the
owner's port-8000 server or `~/.cluaiz`). Script: `first-api.sh`; output: `output.txt` (keys replaced by
`<key>`). Source of every output in `docs/get-started/first-api-request.md`.

| Step | Result |
|---|---|
| `cluaiz pull qwen3:0.6b` | not a catalogue ID: "No matching Cluaiz model ID"; the page uses the full repository ID |
| `cluaiz pull unsloth/Qwen3-0.6B-GGUF:Q4_K_M` | installed as `qwen3-0.6b-gguf-Q4_K_M`, 0.40 GB |
| `GET /health` | 200, `"status":"alive"` |
| chat, `think_mode: off`, `max_tokens: 20` | `"content":"**Paris**"`, `finish_reason: stop` |
| chat, streamed with `include_usage` | usage chunk first, then text deltas, then `[DONE]` |
| wrong model | 400 `model_not_found` |
| `cluaiz token create` while the server runs | the running server still answers without a key (200) |
| after restarting the server | 401 without the key, 200 with it, `/health` 200 without |

Found: `cluaiz token create` changes `permission.json` only; a running server keeps the setting it started
with. CEP `tokens.create` updates the running server but does not set `api_auth.required`. Documented in
`docs/engine/permission.md`, `first-api-request.md` and `troubleshooting.md`; the code is unchanged (auth
behaviour is the owner's design).

Logs, raw results and scripts: [`test/cloud/runs/2026-11-21-run29-docs-first-api-request/`](../cloud/runs/2026-11-21-run29-docs-first-api-request/)

## All logs

| Run | Folder |
|---|---|
| 62 | [`2026-11-24-run62-ollama-chat-plain`](../cloud/runs/2026-11-24-run62-ollama-chat-plain/) |
| 57 | [`2026-11-24-run57-ollama-generate`](../cloud/runs/2026-11-24-run57-ollama-generate/) |
| 37 | [`2026-11-21-run37-ollama-ps`](../cloud/runs/2026-11-21-run37-ollama-ps/) |
| 36 | [`2026-11-21-run36-ollama-library`](../cloud/runs/2026-11-21-run36-ollama-library/) |
| 35 | [`2026-11-21-run35-ollama-tags-keep-alive`](../cloud/runs/2026-11-21-run35-ollama-tags-keep-alive/) |
| 34 | [`2026-11-21-run34-openai-stop-and-limit`](../cloud/runs/2026-11-21-run34-openai-stop-and-limit/) |
| 29 | [`2026-11-21-run29-docs-first-api-request`](../cloud/runs/2026-11-21-run29-docs-first-api-request/) |
