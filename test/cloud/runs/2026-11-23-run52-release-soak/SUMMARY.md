# Run 52: isolated Windows Engine chat soak, partial release coverage (2026-11-23)

This is a partial release test, not an hours-long soak or a full 0.2.0 sign-off. The test client is [`test/release_soak/soak.py`](../../../release_soak/soak.py). It started and restarted the Engine itself, used fixed seeds, wrote each request and response as it happened, sampled process RAM and GPU memory, and saved Engine logs. It did not alter Engine code.

## Setup and provenance

- Windows 11 laptop, Ryzen 7 7435HS, 23.69 GiB RAM, NVIDIA GeForce RTX 3050 A Laptop GPU, 4094 MiB VRAM. Before the first attempted run, `nvidia-smi` showed 2470 MiB free; RAM before that attempt is unknown because the CIM and performance-counter probes failed in the sandbox. After the tests, Win32 `GlobalMemoryStatusEx` showed 7.07 GiB free RAM and `nvidia-smi` showed 2637 MiB free VRAM. These are shared-machine readings, not Engine-only consumption.
- Debug `target/debug/deps/cluaiz.exe`, SHA-256 `F99619FAFFD09744A4427F2B3F2285780D8ED991F6C9C79B8B53C344BB26C40C`. The checkout was `955c815c` when checked. The binary was not rebuilt here; exact source commit of this binary is unknown.
- Isolated home under the workspace `output/release-soak-home`, host `127.0.0.1`, port 8013. Neither the owner's port 8000 server nor `~/.cluaiz` was used. Model: `qwen3.5-0.8b-gguf-Q4_K_M` Q4_K_M, copied from the existing run 49 fixture. The first completed runs had a registry entry pointing at the run 49 model file. Before seed 5203, that entry was changed to the copy in the isolated home; the Engine log for the final run confirms the new path. The model bytes were not changed.
- Initial sandbox attempt could start the Engine but localhost HTTP was forbidden by the sandbox. It has setup/stop events but no scenario result. The completed runs used approved local socket access. No provider keys were loaded or used.

## Recorded results

Five completed invocations used seeds 52, 5201, 5202, 5203 and 5204, with configured durations 0.2, 3, 3, 0.1 and 4 minutes respectively. Seed 5201 overran its target duration because a request waited for its 180-second timeout. The client recorded 14 Engine starts across all attempts and completed invocations. `raw/events.jsonl` gives the exact starts, stops, seeds and per-run totals. The total is **61/64 scenario checks passed**, with **3 failures**. This counts scenarios, not individual HTTP requests: a multi-turn case sends 10 chats and a concurrent case sends 3 chats.

| Scenario | Pass/total | Check made |
|---|---:|---|
| Single chat | 7/7 | HTTP 200, reply schema, answer includes 4 |
| Streaming chat | 6/6 | HTTP 200, SSE data and `[DONE]` |
| Ten-turn chat | 6/6 | Ten replies with valid choices; answer quality per turn was not scored |
| Three simultaneous users | 6/7 | Three valid answers; one thinking/default request timed out at 180 seconds |
| Malformed JSON | 5/5 | HTTP 400 or 422 |
| Unknown model | 6/6 | HTTP 400 or 404 |
| Empty messages | 6/6 | HTTP 400 or 422 |
| `n=2` | 6/6 | HTTP 400 with field named |
| `logprobs=true` | 7/7 | HTTP 400 with field named |
| Max completion tokens | 2/2 | Reported completion tokens at most 5 |
| Long marker prompt | 0/2 | HTTP 200 but empty answer, without marker 4917 |
| Thinking enabled | 2/2 | HTTP 200 and reply schema; answer quality was not scored |
| Client closes a stream | 2/2 | First SSE data received, then client socket closed; server recovery was not separately asserted |

Seed 5202 used `think_mode: off` and returned `passed=31 failed=0`. The earlier seed 5201 used the model's default thinking path and returned `passed=2 failed=1`. Seed 5204 returned `passed=24 failed=2`. The raw `checks.jsonl`, `requests.jsonl`, `responses.jsonl`, `events.jsonl`, `memory.jsonl`, and `engine.log` are the evidence; they are written during the run. The logs replace the session token, username and IPv4 addresses. A local scan found none of the tested secret markers in the saved files.

## Open observations

1. P2 answer quality: the long marker prompt returned an empty answer twice. See [`bugs/long-prompt-empty-answer.md`](bugs/long-prompt-empty-answer.md).
2. P2 latency under this setup: one of three concurrent thinking/default chats exceeded 180 seconds. The 3-user think-off control passed four concurrent scenarios. Root cause is unknown. See [`bugs/concurrent-think-timeout.md`](bugs/concurrent-think-timeout.md).

## Not tested in this run

This client did not run for hours. It did not cover stop or logit bias, RAM prompt-cache proof, mid-chat Engine restart, model switch under load, five-image sequences, OCR/video, speech, embeddings, ingest, tools, MCP, approval modes, files tool, providers, Ollama, CEP, workflows, most CLI commands, Linux/macOS, or release builds. Run 49 and run 50 contain earlier evidence for many of these paths on different builds and setups; they are not counted as Run 52 passes. No leak trend or performance regression is claimed from these short, shared-machine runs.

To rerun the current client, prepare a fresh isolated home with a registered chat model and use the command in `test/release_soak/soak.py`'s module header. Keep port 8013 free. `--seed` reproduces scenario order; `--minutes` controls duration. The exact model registry and binary hash must be recorded with the new result.
