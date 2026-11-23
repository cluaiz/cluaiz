# Run 49: release end-to-end check of the Engine on the Windows laptop (2026-11-23)

Windows 11 (build 26300), Ryzen 7 7435HS, 23.7 GB RAM, RTX 3050 4 GB. Debug `cargo build -p cmd` of `main`
at `19c77783` plus the three fixes below (committed as `75826a83`, `4b5320fd`, `3dd5cd14`); the
llama.cpp submodule in the working tree was at `8676a916e` (another agent's uncommitted pointer, not
`a2d0fbed9` of HEAD). `target\debug\cluaiz.exe` was locked by another server, so the binary under test is
`target\debug\deps\cluaiz.exe` (same link output). Isolated home `%LOCALAPPDATA%\cluaiz-test\run49-e2e`, port
8013, host 127.0.0.1, auth settings left at their defaults.

Models pulled with `cluaiz pull` into the home (1.3 GB): `unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M` (with mmproj),
`ggml-org/embeddinggemma-300M-GGUF`, `onnx-community/Kokoro-82M-v1.0-ONNX:uint8`,
`onnx-community/moonshine-tiny-ONNX:int8`. Hard-linked from other test homes (no new disk): Ternary-Bonsai-2-4B
with its mmproj (vision control) and Qwen3 1.7B IQ4_XS (plain KV, for the RAM prompt cache).

Scripts: `cli_checks.sh` (CLI), `release_e2e.py` (OpenAI, Ollama and CEP over HTTP), `tools_checks.py`
(plugin, MCP, workspace, flow run), `run49-check.flow.yaml`; existing `test/cep/journeys.py` and
`test/cep/mcp_journey.py`. Plan: `.agent/plans/engine/2026-11-23-release-e2e.md` (git-ignored).

## Results per feature

| Area | Check | Result | Raw |
|---|---|---|---|
| pull | 4 models, ids registered | pass (65 s, 172 s, 17 s, 184 s) | `raw/pull.txt` |
| list, show, status | models, plan per device, hardware | pass | `raw/cli_list.txt`, `cli_show.txt`, `cli_status.txt` |
| config, permission, token, provider, storage | read commands | pass | `raw/cli_config_*.txt` and others |
| model set-chat / set-vector | slot saved | pass | `raw/cli_set_*.txt` |
| run one prompt | "What is 2+2?" | pass, `4` | `raw/cli_run_prompt.txt` |
| bench | pp64 / tg16 | pass, 3181.7 / 122.1 t/s, Full GPU | `raw/cli_bench.txt` |
| ps | loaded model through the server on 8013 | pass | `raw/cli_ps.txt` |
| plugin / mcp install, list | `math` (WASM), `filesystem` (MCP) from the registry | pass | `raw/cli_plugin_*.txt`, `cli_mcp_*.txt` |
| flow validate, save, run, logs | `run49-check` (wait 8 s, then ask) | pass (save needs the y answer; run 15.3 s) | `raw/cli_flow_save_yes.txt`, `cli_flow_run_after_save.txt` |
| ingest (CLI) | Markdown file with a GGUF embedding model | **fail, fixed** (`3dd5cd14`) | `raw/cli_ingest.txt`, `cli_ingest_after_fix.txt` |
| agent headless | `agent -p ... --output-format json` reads notes.txt | pass, "The secret number ... is 4917" | `raw/cli_agent_headless.txt` |
| rm | removes a model | pass | `raw/cli_rm.txt` |
| /health, /v1/models | | pass | `raw/http_checks.jsonl` |
| auth | no token | 200, as designed: `api_auth.required` is false by default; not changed | same |
| chat, streaming with usage chunk | | pass | same |
| stop (string and list), max_completion_tokens | | pass | same |
| n > 1, logprobs refused with 400 | | pass | same |
| logit_bias | "Yes" +100 forces Yes, "No" -100 removes No, unknown id 400 | pass | same |
| tools (function calling) | `get_weather` with `{"city":"Paris"}` | pass | same |
| approval_mode | `strict` 200, bogus 400, global mode unchanged | pass | same |
| multi-turn KV reuse | TTFT 1970 ms then 253 ms | pass | same |
| RAM prompt cache (`CLUAIZ_PROMPT_CACHE_MB=256`) | A, B, A on Qwen3 1.7B | pass, "Restored an earlier prompt: 1234 tokens shared", 1233 from the cache | `raw/prompt_cache_qwen3.txt`, `serve3_prompt_cache.log` |
| vision | red image, two part orders | **fail, open** (see below) | `raw/vision_*.txt` |
| embeddings, TTS, STT | 768 dims; 171,644-byte WAV; transcript exact | pass | `raw/http_checks.jsonl` |
| /v1/ingest | Markdown data URI | pass | same |
| Ollama routes | version, tags (752.39M from the GGUF header), show (404 unknown), chat, chat stream, generate, embed, ps | pass | same |
| CEP reads | 18 methods | pass | same |
| CEP system.events | empty long poll waits 1.5 s; a poll returns `task.done` after 1.0 s | pass | same |
| CEP config round trip | | pass | same |
| CEP live port (App problem 4) | `CLUAIZ_PORT=8013`, 8000 saved | **fail, fixed** (`75826a83`): 8000 before, 8013 after | `raw/a3_before_old_exe.txt`, `http_checks.jsonl` |
| WASM plugin approval (App problem 6) | math in sandboxed mode, CEP and chat | pass, no question, 1028 | `raw/tools_checks.jsonl` |
| files tool and the home (App problem 2) | no folder open: `.`, `..`, `../session.token` | pass | same |
| | `workspace.set <home>` | **fail, fixed** (`4b5320fd`): accepted and listed session.token before, refused after | `raw/tools_checks_pass1.jsonl`, `tools_checks.jsonl` |
| flows.run_get running step (App problem 1) | step while it runs | pass, `('pause', 'wait', 'running')` | `raw/tools_checks.jsonl` |
| `test/cep/journeys.py` | discovery, batch, tasks, audit, latency | 8 / 8 pass | `raw/journeys_cep.txt` |
| `test/cep/mcp_journey.py` | `/mcp` and `cluaiz mcp-server` | 14 / 14 pass | `raw/journeys_mcp.txt` |
| `test/cep/wave_b/*_http_journey.py` | | not run: the scripts look for `<home>/engine/config`, the home now keeps `config/` at its root | `raw/journeys_wave_b_*.txt` |

`raw/http_checks_pass1.jsonl` and `pass2` are the first runs; their failures for auth, logit_bias, TTS,
STT, system.events and two CEP reads were mistakes in the script (expected 401 with auth off, a prompt the
model already answered "No", the speech reply's `output.audio_data`, the `after` / `next` cursor, a missing
`chat_id`), corrected before the final `raw/http_checks.jsonl`.

## Open failure: vision on a 4 GB GPU

Qwen3.5-0.8B with its mmproj, a red 448x448 PNG, temperature 0.

1. **Every image request after the first fails with HTTP 500 "Insufficient live VRAM for media request
   workspace" when the first request of the server was an image** (`raw/vision_vram_root_cause_2.txt`): the
   first answer is "The image is red.", the next three are 500. Free VRAM after the first was 1503 MiB.
   When a text chat ran first, three image requests in a row passed (`raw/vision_vram_root_cause.txt`).
   The refusal is `engine/llama/src/native/session.rs:427-435`: each request needs the projector's
   whole workspace (`media.projector_workspace_bytes`, 1 GiB) plus the device buffer free right now,
   while a projector loaded on the GPU already holds its weights and, after its first encode, its compute
   buffers (it is loaded with `warmup = false`, `engine/llama/src/native/media.rs:74`). There is no fallback:
   the resident projector is reused (`session.rs:301-321` checks only RAM) and never moved to the CPU.
   The same 500 came after embedding, speech and transcription models were loaded (848 MiB free,
   `raw/prompt_cache_and_vram.txt`). Not fixed here: it is the llama crate's VRAM rule, whose tests this
   run may not run. A possible fix in the existing design: when the request check fails, reclaim the idle
   projector and plan it again (as `ensure_projector` already does for RAM), or do not count the projector's
   own allocations twice.
2. **With the text part before the image part, the model said it cannot see images** on the first server
   of the run (4 of 4 times, `raw/vision_order_probe.txt`), while the image-first order answered "red", and
   official `llama-mtmd-cli` b12428 answered "red" for both orders (`raw/vision_official_mtmd*.txt`).
   Ternary-Bonsai-2-4B answered "red" in both orders. On later servers the text-first order answered "red"
   (`raw/vision_vram_root_cause_2.txt`). Cause ?; the projector placement (CPU or GPU) chosen from the free
   VRAM at its load was not logged.

## Other observations

- `cluaiz permission` prints the saved `port 8000` while the server runs on `CLUAIZ_PORT`; it is the
  setting, so not changed.
- The flow's `ask` step took 39.6 s and 84.6 s on the first two runs (the 0.8B model thinks; the step has
  no thinking setting in the file), 15.3 s on the third.
- With tools installed, every chat logged `[NativeTools] Tool template failed (...); tools go inline`; the
  answers were right.

## What this run does not prove

- Linux, macOS, release builds and the installers; Vulkan and CPU packs.
- Remote model providers (no keys used), OCR, video, audio input in chat, scanned PDFs.
- `CLUAIZ_MOE_CACHE` and the expert prefetch Auto rule (no MoE model was loaded; runs 42-46).
- The CLI ingest fix with an ONNX text embedding model (none installed); the image path was not changed.
- Concurrency, and speed against other engines.
