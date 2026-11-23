# Run 53: release API, CLI, tools and media checks (2026-11-23)

This is a functional follow-up to Run 52, not a complete 0.2.0 release sign-off. Windows 11 laptop, Ryzen 7 7435HS, 23.69 GiB RAM, RTX 3050 4 GB. The main binary was the existing debug `target/debug/deps/cluaiz.exe`, SHA-256 `F99619FAFFD09744A4427F2B3F2285780D8ED991F6C9C79B8B53C344BB26C40C`. The checkout before tests was `d8df40c6`; the exact source commit of that prebuilt executable is unknown. CLI `--version` and MCP returned `0.1.0 (pre-release)`, not 0.2.0. A second existing debug binary, SHA-256 `66B497B36D2780957FEDC8ECB92418C0BD7A304DD8D0AB79ED6C4CDF2D4868D9`, was used only for the Ollama control.

The Engine used port 8013 or 8014 and isolated `output/release-soak-home`; neither the owner's port 8000 server nor `~/.cluaiz` was used. Four test models were registered inside that home: Qwen3.5 0.8B Q4_K_M with projector, EmbeddingGemma 300M Q8_0, Kokoro 82M UINT8, and Moonshine Tiny INT8. The three non-chat packages and projector were copied from the existing run 49 fixture; no provider keys were used. The `math` plugin and `filesystem` MCP were installed into this test home by the CLI during this run. The binary was not rebuilt and Engine code was not changed.

## Results

| Area | Valid result | Evidence and limit |
|---|---:|---|
| OpenAI-compatible, Ollama and CEP API matrix | 29/30 | `raw/api_checks.jsonl`; Ollama generate failed answer correctness |
| Ollama generate controls | 0/3 | Same wrong answer with clean Engine CWD, fresh config home, and second debug binary; `raw/api_checks_*` |
| CLI initial matrix | 12/14 | `raw/cli_checks.jsonl`; `config` was a bad harness assertion and `storage` needed a running server |
| CLI corrected storage and config | 2/2 | `raw/cli_checks_retry.jsonl`, isolated server running |
| CLI flow validate/save/list/run/ingest | 5/5 | Same retry file; the flow run succeeded in 6,528 ms with two steps, and ingest generated two real vectors |
| CLI flow logs | 1/1 corrected retry | Earlier assertion expected the flow name, but the CLI prints run id and status; another retry without a server failed. Server-backed retry passed. All attempts remain in `raw/cli_checks_retry.jsonl`. |
| Headless agent | 1/1 | Read the test workspace's `notes.txt` and returned 4917; retry file |
| Installed math, filesystem, flow-step checks | 8/8 | `raw/tools_checks_installed_tools.jsonl`; includes a refusal to set the Engine home as workspace and a running step with an id |
| CEP journey | 8/8 | `raw/cep_journeys_isolated.txt`; 116 discovered methods, batch, tasks and audit checks |
| MCP over HTTP and stdio | 14/14 | `raw/mcp_journey_isolated.txt`; invalid write and invalid arguments were refused |
| Five 448x448 images | 5/5 | `raw/vision/run53_five_images.txt`; all answers Red, text-first and image-first alternating, GPU free 885-889 MiB after requests |
| One red video | 0/1 | HTTP 400, native video decoder unavailable because the media/OCR pack is absent; `raw/video_check.jsonl` and `raw/responses.jsonl` |

The API matrix covered chat, streaming usage, stop, max tokens, `n`/logprobs refusal, logit bias, function calls, per-chat approval mode validation, two image part orders, multi-turn cache, embeddings, TTS, STT, ingest, Ollama version/tags/show/chat/embed/ps, 18 CEP reads, live port, events and config round trip. Each API check's request, reply, duration and result is in `raw/api_checks.jsonl`; the reused Run 49 driver trims long request and reply bodies at 1,500 characters, so this is not a complete wire transcript. The initial matrix's one failure was `/api/generate`: the capital-of-France prompt returned a Cluaiz description. See [`bugs/ollama-generate-unrelated-answer.md`](bugs/ollama-generate-unrelated-answer.md).

The video request used the documented `video_url` shape and a generated 2,246-byte red MP4. Its failure is recorded in [`bugs/video-decoder-unavailable.md`](bugs/video-decoder-unavailable.md). This setup lacked the native media pack; the result does not establish behavior of a packaged release with that pack.

## Test-harness incident and exclusions

The first CEP journey command used PowerShell's read-only `$HOME` variable by mistake. Its printed 8/8 is **excluded** and saved as `raw/cep_journeys_invalid_home.txt`. It created test files in the Windows profile root. Their creation times were verified and those newly created files and folders were removed. A pre-existing `cluaiz_Core.log` in that profile had test lines appended and was preserved. The correct command used `$testHome`; its isolated 8/8 result is the one counted above.

Before considering a long soak, this shared laptop had 3.75 GiB free RAM and 2256 MiB free VRAM. A two-hour leak or speed claim would be confounded by the other workload, so this run did not start it. Provider checks were not run; no provider keys were loaded. OCR was not tested because the docs say the OCR pack is absent from release packages. Small-then-large image, audio input to chat, model switch during chat, restart mid-chat, all 30 requested workflows, approval mode full-access tightening, CLI pull/rm/ps, and release/Linux/macOS builds remain untested in this run. Run 49-52 contain earlier evidence for some paths, but their results are not counted here.

All saved Run 53 files were scanned for the isolated session token, username, IPv4 loopback address and common provider-key prefixes before commit. No test result in this folder contains a provider key. Reproduction scripts are `suite.py`, `cli_matrix.py`, and `video_probe.py`; they require an isolated home and ports other than 8000 and 8011. The original Run 49 and Run 50 drivers are linked from those scripts and remain unchanged.
