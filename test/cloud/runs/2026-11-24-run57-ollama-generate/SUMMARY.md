# Run 57: Ollama `/api/generate`, long prompt, chat KV size (2026-11-24)

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

## Results

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

## What was wrong

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

## Fixes

| Fix | Change | Proof |
|---|---|---|
| A: `/api/generate` is a plain prompt | `ExternalChatRequest.plain_prompt` (`#[serde(skip)]`, set only by `ollama.rs` for generate): no notes, no project agent files, no tools; generate's `tools` field is not passed on | 17 prompt tokens and Paris in both homes; `/v1/chat/completions` tools still work |
| B: the default workspace is its own root | `agent_folders` in `agents/src/workspace/mod.rs` (uses `EnvironmentManager::is_unchosen_workspace`); a folder the user opens inside the same checkout still reads it | in-repo `/v1` request 4,056 -> 883 tokens, project root the home's `workspace`; unit test |
| Bug 2: no call as reasoning | `non_streaming.rs`: at `Ended::StepLimit` only the text before `<tool_call>` is the answer | `reasoning_content` gone in both homes |

## Not done

| Item | Why it matters | Needs |
|---|---|---|
| `long_input` still fails on the 0.8B model: it calls tools until the repeat limit and says nothing | an empty answer to a long plain prompt | a decision on offering tools for messages that do not ask for any, or a final answer step after the repeat limit |
| Streaming path at the step limit not checked | the same leak may exist there | a streamed `long_input` |
| `/api/chat` still gets the Engine's additions | Ollama chat clients see Engine functions | owner decision (see `api/src/handlers/README.md`, Pending) |
| api and runtime test suites not run | owner rule for this task | CI with the CPU backend |
