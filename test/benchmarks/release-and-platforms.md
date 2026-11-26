# Release builds on every platform: install, features, end-to-end, soak

## Answer

| Area | Latest result | Run |
|---|---|---|
| Linux release build (Debian 13, CUDA, L4): end-to-end and 2-hour soak | HTTP 40 / 41 (`chat_vision` text first, fixed in run 76), tools 8 / 8, CEP 8 / 8, MCP 14 / 14; soak 210 / 249 (`max_tokens` fixed in run 78, `long_input` 180 s timeouts under load) | 72 |
| Release API, CLI, tools and media matrix (Windows) | API 29 / 30 (Ollama generate, fixed in run 57); video needs the native media pack | 53 |
| Release end-to-end on the Windows laptop | 3 bugs fixed; vision VRAM issue fixed in run 50 | 49 |
| 1 vCPU / 3.8 GB VPS, deep test with a resource log | 9 chat models, speech, load, agent, flows; 2 CLI timeouts on a busy 1 vCPU | 19 |
| 1 vCPU VPS features | 16 / 17 feature checks | 18 |
| Linux L4 release v0.1.0 | features and journeys pass | 8 |
| Linux CPU release v0.1.0 | features and journeys pass; `flow draft` needs more than 4096 context | 7 |
| MacBook Air M5 16 GB, release v0.1.0 | installs and chats at plain llama.cpp's speed for the same settings | Mac M5 |
| Install and CPU runs on clean machines: Linux x64 / ARM64, Windows Server, macOS | every supported system installed and answered (tg128 2.1-23.9 t/s); glibc below 2.38 stops with a message | 6 |

Below: every current run of this test type, newest first, in full.

<a id="run-72"></a>

## Run 72 (2026-11-25)

#### Setup

| Item | Value |
|---|---|
| Machine | GCP g2-standard-8 (the run 48 VM): Intel Xeon 2.20 GHz, 8 vCPU, 31 GiB RAM, NVIDIA L4 23 GB, driver 620.70.06; Debian 13, kernel 6.12.117, glibc 2.41 (`raw/e2e/machine.txt`) |
| Build | `build.sh`: `cargo build -p cmd --release --locked` of `7498b6b4` (llama.cpp submodule `96b0cfb2d`), 1,473 s, rustc 1.100.0, CUDA toolkit 13.2 with `CUDA_ARCHITECTURES=89` (L4 only), then `.github/scripts/package_core.sh` and the CUDA driver + runtime libraries beside it, as the release packs install them. Not a CI artifact (CI builds the default arch set with the 12.8.1 / 13.4.1 containers). Binary SHA-256 `06fbf091...82012916` (`raw/build.log`, `raw/e2e/machine.txt`) |
| Server | `cluaiz serve` from the package folder, fresh isolated `CLUAIZ_HOME` (`~/run72/home`), `CLUAIZ_PORT=8013`, `CLUAIZ_HOST=127.0.0.1`; auth settings left at their defaults (not required, not changed) |
| Models | pulled with `cluaiz pull`: Qwen3.5-0.8B Q4_K_M (chat), embeddinggemma-300M Q8_0, Kokoro-82M UINT8, moonshine-tiny INT8 (`raw/e2e/pulls.txt`) |
| Tools in the home | math plugin, filesystem MCP (`cli_plugin_install`, `cli_mcp_install`) |
| Scripts | `e2e.sh` (CLI `cli_checks.sh`, HTTP `release_e2e.py`, `tools_checks.py`, the CEP and MCP journeys of `test/cep`), `e2e_node.sh` (second pass after Node.js 20 / npm 9 were installed, so the filesystem MCP could start), `soak.sh` (unchanged `test/release_soak/soak.py` + `bg_users.py` + `sampler.sh`), `analyze.py`, `scrub.sh` |
| Rerun | on the VM: `bash build.sh <repo> <out>`; `bash e2e.sh <package> <repo> <out>/e2e`; `bash e2e_node.sh <package> <out>/e2e`; `bash soak.sh <package> <repo> <out>/soak 120 7201`; `python3 analyze.py <out>/soak` |

#### Results: end-to-end checks (`raw/e2e/`)

| Part | Result |
|---|---|
| CLI, 26 commands (`cli_summary.txt`) | 23 exit 0. `flow save` exit 1: non-interactive, the permission prompt read no answer and the flow was not saved (by design); `flow run` and `flow logs` then exit 1 (no such flow). With `y` answered: saved, `flow run` succeeded in 17,524 ms, 2 steps (`cli_flow_save_yes.txt`, `cli_flow_run_after_save.txt`). `run` answered `4`; headless `agent` read notes.txt and answered 4917; `bench` pp64 5,205 t/s, tg16 258 t/s (Full GPU, 1 run) |
| HTTP, 41 checks (`http_summary.txt`) | 40 / 41. The one failure is `chat_vision` text first: red 448x448 image, the model called `read_item` for "image" and did not name the colour; image first answered "solid red". Same bug as run 76 (fixed there in `0961d0ee`, after this build). `long_input_option_b` 3 / 3 real answers with the marker, 10.5-10.6 s (filesystem MCP not running in this pass) |
| HTTP second pass with Node.js (`http_summary_node.txt`) | 4 / 5: `chat_tools`, `vision_tools_call`, `json_schema_with_tools` pass; `long_input_option_b` 3 / 3 with the marker, 127.5-127.7 s (now with the filesystem MCP's tools in the loop); `chat_vision` text first fails as above |
| Tool and permission checks (`tools_summary.txt`, `tools_summary_node.txt`) | first pass 7 / 8 (`A2_files_tool_default_workspace`: listing null, because without `npx` the filesystem MCP could not start); second pass 8 / 8. The `..` listing and `../session.token` read did not leak the token; `workspace.set` to the home is refused; math runs without an approval prompt in sandboxed mode; a waiting flow step shows as running |
| CEP journeys (`journeys_cep.txt`) | 8 / 8 (116 methods in `rpc.discover`; loopback `tasks.list` p50 0.19 ms, p95 0.23 ms) |
| MCP journeys (`journeys_mcp.txt`) | 14 / 14 |

#### Results: soak (`raw/soak/`, `raw/soak/analysis.md` from `analyze.py`)

120 minutes, seed 7201, `soak.py` with 4 users and a server restart every 2 sweeps (10 server starts, 20 sweeps), plus
2 extra users from `bg_users.py` the whole time.

| Scenario | Pass / total | Note |
|---|---|---|
| bad_json, cancel_stream, concurrent, empty, logprobs, multi, n, single, stream, think_on, unknown_model | 210 / 210 | |
| `max_tokens` (`max_tokens` 48 + `max_completion_tokens` 5, pass = `completion_tokens` <= 5) | 0 / 19 | 19 / 19 replies HTTP 200 with `completion_tokens` 192, `finish_reason` length, 196 content chars: `max_completion_tokens` was ignored and the limit applied per tool step, not to the whole loop. Fixed in `95476879`, checked in run 78 |
| `long_input` (marker + 1,500 `word`, `max_tokens` 48, 180 s client timeout) | 0 / 20 | 19 client timeouts at 180 s; 1 reply after 164 s: prompt 6,212 tokens, 48 completion tokens, content was `<tool_call>` text, no marker. In the e2e pass without load the same request answered 3 / 3 (127.5-127.7 s with the filesystem MCP) |
| **total** | **210 / 249** | counts rechecked from `raw/soak/checks.jsonl` on 2026-11-26 |

Extra users (`bg.jsonl`, 207 requests): stream 52 pass, think_off 68 pass, think_on 67 pass; all 20 failures ended in
a server restart window (8 + 8 + 4), none outside one; every pass had the expected answer. 41 requests took more than
60 s (max 371 s); 40 of them overlapped a `long_input` request.

Memory (`sampler.jsonl`, every 10 s, 722 samples): each server process lived 9.6-13.4 minutes; RSS 409-1,378 MiB
(last sample 1,252-1,366 MiB in every process), GPU memory 1,452-1,458 MiB once loaded, threads 12-20, open files
38-134. No process lived long enough to show a slow leak.

#### What this proves

- The release layout built from source on Debian 13 with CUDA runs every feature group on the L4: chat, stream,
  tools, JSON schema, thinking, vision with tools, embeddings, TTS, STT, Ollama routes, CEP, MCP, flows, CLI agent.
- Under two hours of mixed load with restarts, no crash outside planned restarts; failed requests only in restart
  windows, and two known bugs: the `max_tokens` budget (fixed later, run 78) and the text-first vision answer
  (fixed later, run 76).

#### What this does not prove

- Not the CI release artifact (own build, L4 arch only, CUDA 13.2); no CPU-only or Vulkan pack; one model size.
- `long_input` under load: whether it would finish with a longer client timeout, and why the one reply was call
  markup, is not analysed here (runs 70 / 73 cover the loop's final answer without load).
- Memory over hours in one process (restarts every about 12 minutes).

#### Files

`raw/build.log`; `raw/e2e/` (machine, pulls, CLI outputs, HTTP and tools JSONL + summaries, journeys, serve logs and
highlights); `raw/soak/` (soak.py's `checks.jsonl`, `events.jsonl`, `requests.jsonl`, `responses.jsonl`,
`memory.jsonl`, `engine.log.gz` (gzipped, 6.6 MB plain), `bg.jsonl`, `sampler.jsonl`, `analysis.md`, GPU snapshots).
Scrubbed on the VM with `scrub.sh` before copying (user name `<user>`, session token, VM addresses and cloud
project id: 0 files left with any of them); no token in any file.

#### Pending

| Item | Needs |
|---|---|
| `long_input` under load with a longer client timeout | a soak rerun with the fixed build |
| Soak with the run 76 / 78 fixes | a new Linux soak (VM time) |
| CI release artifact on Linux | `install.sh` of the next pre-release on the VM |

Logs, raw results and scripts: [`test/cloud/runs/2026-11-25-run72-linux-release-soak/`](../cloud/runs/2026-11-25-run72-linux-release-soak/)

<a id="run-53"></a>

## Run 53 (2026-11-23)

This is a functional follow-up to Run 52, not a complete 0.2.0 release sign-off. Windows 11 laptop, Ryzen 7 7435HS, 23.69 GiB RAM, RTX 3050 4 GB. The main binary was the existing debug `target/debug/deps/cluaiz.exe`, SHA-256 `F99619FAFFD09744A4427F2B3F2285780D8ED991F6C9C79B8B53C344BB26C40C`. The checkout before tests was `d8df40c6`; the exact source commit of that prebuilt executable is unknown. CLI `--version` and MCP returned `0.1.0 (pre-release)`, not 0.2.0. A second existing debug binary, SHA-256 `66B497B36D2780957FEDC8ECB92418C0BD7A304DD8D0AB79ED6C4CDF2D4868D9`, was used only for the Ollama control.

The Engine used port 8013 or 8014 and isolated `output/release-soak-home`; neither the owner's port 8000 server nor `~/.cluaiz` was used. Four test models were registered inside that home: Qwen3.5 0.8B Q4_K_M with projector, EmbeddingGemma 300M Q8_0, Kokoro 82M UINT8, and Moonshine Tiny INT8. The three non-chat packages and projector were copied from the existing run 49 fixture; no provider keys were used. The `math` plugin and `filesystem` MCP were installed into this test home by the CLI during this run. The binary was not rebuilt and Engine code was not changed.

#### Results

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

The API matrix covered chat, streaming usage, stop, max tokens, `n`/logprobs refusal, logit bias, function calls, per-chat approval mode validation, two image part orders, multi-turn cache, embeddings, TTS, STT, ingest, Ollama version/tags/show/chat/embed/ps, 18 CEP reads, live port, events and config round trip. Each API check's request, reply, duration and result is in `raw/api_checks.jsonl`; the reused Run 49 driver trims long request and reply bodies at 1,500 characters, so this is not a complete wire transcript. The initial matrix's one failure was `/api/generate`: the capital-of-France prompt returned a Cluaiz description. See [`bugs/ollama-generate-unrelated-answer.md`](../cloud/runs/2026-11-23-run53-api-matrix/bugs/ollama-generate-unrelated-answer.md.txt).

The video request used the documented `video_url` shape and a generated 2,246-byte red MP4. Its failure is recorded in [`bugs/video-decoder-unavailable.md`](../cloud/runs/2026-11-23-run53-api-matrix/bugs/video-decoder-unavailable.md.txt). This setup lacked the native media pack; the result does not establish behavior of a packaged release with that pack.

#### Test-harness incident and exclusions

The first CEP journey command used PowerShell's read-only `$HOME` variable by mistake. Its printed 8/8 is **excluded** and saved as `raw/cep_journeys_invalid_home.txt`. It created test files in the Windows profile root. Their creation times were verified and those newly created files and folders were removed. A pre-existing `cluaiz_Core.log` in that profile had test lines appended and was preserved. The correct command used `$testHome`; its isolated 8/8 result is the one counted above.

Before considering a long soak, this shared laptop had 3.75 GiB free RAM and 2256 MiB free VRAM. A two-hour leak or speed claim would be confounded by the other workload, so this run did not start it. Provider checks were not run; no provider keys were loaded. OCR was not tested because the docs say the OCR pack is absent from release packages. Small-then-large image, audio input to chat, model switch during chat, restart mid-chat, all 30 requested workflows, approval mode full-access tightening, CLI pull/rm/ps, and release/Linux/macOS builds remain untested in this run. Run 49-52 contain earlier evidence for some paths, but their results are not counted here.

All saved Run 53 files were scanned for the isolated session token, username, IPv4 loopback address and common provider-key prefixes before commit. No test result in this folder contains a provider key. Reproduction scripts are `suite.py`, `cli_matrix.py`, and `video_probe.py`; they require an isolated home and ports other than 8000 and 8011. The original Run 49 and Run 50 drivers are linked from those scripts and remain unchanged.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-23-run53-api-matrix/`](../cloud/runs/2026-11-23-run53-api-matrix/)

<a id="run-19"></a>

## Run 19 (2026-11-16)

| | |
|---|---|
| Machine | 1 vCPU (AMD EPYC 9354P, AVX-512), 3.8 GB RAM, no swap, no GPU, Ubuntu 24.04; a website container runs beside it (about 0.6 GB of the machine's memory in use before Cluaiz starts) |
| Cluaiz | v0.1.0 re-published release (tag `eb11d47d`); server on 127.0.0.1:8011 in a 2.9 GB memory scope |
| Scripts | [`vps19/`](../cloud/remote/vps19/): `run19.sh` (phases), `sampler.py` (one row per second), `workloads.py`, `analyze.py` |
| Records | [`results-run19/`](../cloud/runs/2026-11-16-run19-vps-deep/results-run19/): `resources.csv` (1,556 seconds), `phases.md`, `results.jsonl`, `steps.log`, every log |

The first attempt ([`results-run19-attempt1/`](../cloud/runs/2026-11-16-run19-vps-deep/results-run19-attempt1/)) is not used: run 18's server was still
running, so the planner saw too little free RAM and most phases failed. The scripts now stop the server by the
PID that listens on its port.

#### Memory and CPU per phase

From [`phases.md`](../cloud/runs/2026-11-16-run19-vps-deep/results-run19/phases.md.txt). "Anon" is memory the Cluaiz processes really hold; "mapped" is the
model file mapped into memory, which the kernel can drop and read again. Machine memory includes the website
and the page cache that counts as used.

| Phase | Machine memory peak | Cluaiz anon peak | Cluaiz mapped peak | Cluaiz CPU avg |
|---|---|---|---|---|
| Before Cluaiz | 619 MB | 0 | 0 | 0% |
| Server idle, no model | 634 MB | 9 MB | 35 MB | 0% |
| Qwen3 0.6B chat | 1.33 GB | 741 MB | 414 MB | 92% |
| Llama 3.2 1B chat | 1.24 GB | 661 MB | 813 MB | 93% |
| Qwen3 1.7B chat | 1.58 GB | 971 MB | 1.10 GB | 92% |
| Llama 3.2 3B chat | 2.23 GB | 1.63 GB | 1.97 GB | 91% |
| Qwen3 4B chat | 2.59 GB | 1.93 GB | 2.42 GB | 90% |
| Speech (TTS + 3 STT models) | 2.34 GB | 1.71 GB | 1.21 GB | 97% |
| 3 minutes of everything at once | 2.60 GB | 2.00 GB | 1.10 GB | 98% |

One request uses the whole vCPU. The website answered every check during the whole run.

#### Chat models on the CPU (1 thread)

`cluaiz bench`, prompt 512, generation 128, 2 runs; chat through the server, 3 requests of up to 64 tokens.

| Model (Q4_K_M unless named) | File | Planner RAM | Prompt t/s | Generation t/s | First token, cold / warm |
|---|---|---|---|---|---|
| Qwen3 0.6B | 379 MB | 1.09 GB | 116.0 | 36.3 | 1.9 / 0.2 s |
| Llama 3.2 1B | 771 MB | 1.11 GB | 61.6 | 21.5 | 3.5 / 0.4 s |
| Gemma 3 1B | 769 MB | 1.35 GB | 60.7 | 14.5 | 2.5 / 0.4 s |
| Qwen3 1.7B | 1.1 GB | 1.55 GB | 32.9 | 14.5 | 3.6 / 0.5 s |
| Llama 3.2 3B | 1.9 GB | 2.35 GB | 18.8 | 8.8 | 9.9 / 1.1 s |
| Phi-4-mini | 2.4 GB | 2.35 GB | 17.1 | 6.4 | 10.3 / 1.0 s |
| Qwen3 4B | 2.4 GB | 2.91 GB | 12.9 | 6.2 | 13.8 / 1.3 s |
| Bonsai 4B Q1_0 | 546 MB | 1.12 GB | 6.4 | 5.8 | 4.4 / 2.3 s |
| Ternary-Bonsai-2-4B Q2_0 | 1.8 GB (with mmproj) | 1.77 GB | 5.6 | 4.3 | 6.2 / 4.0 s |
| Qwen3 8B | 4.7 GB | 5.27 GB | refused: "3.66 GB short", with the reason | - | - |

#### Speech (ONNX) and embeddings

Real-time factor = processing time / audio length; below 1 is faster than real time. Word recall = share of the
spoken words found in the transcript.

| Model | 3.6 s audio | 6.6 s audio | 10.6 s audio | Word recall |
|---|---|---|---|---|
| Kokoro-82M uint8 (TTS) | 1.16 (first call) | 0.53 | 0.55 | - |
| moonshine-tiny int8 (STT) | 0.29 | 0.14 | 0.09 | 1.00 / 0.93 / 0.76 |
| moonshine-base int8 (STT) | 0.32 | 0.21 | 0.16 | 1.00 / 0.93 / 0.97 |
| whisper-base int8 (STT) | 0.65 | 0.33 | 0.24 | 1.00 / 0.79 / 1.00 |

Embeddings, embeddinggemma-300M Q8_0: 200 sentences in 12.4 s (16.1 per second), 768 dimensions.
`cluaiz ingest`: 141 bytes in 33 ms, 35 KB in 31 ms (text and chunks; no vectors without a vector model set).

#### Load

Requests at once, short answers (up to 48 tokens):

| Model | 1 | 2 | 4 | 8 |
|---|---|---|---|---|
| Qwen3 0.6B, total t/s | 4.5 (included a model switch) | 27.5 | 27.2 | 27.7 |
| Qwen3 0.6B, slowest request | 3.8 s | 1.4 s | 3.5 s | 5.9 s |
| Llama 3.2 3B, total t/s | 3.3 (included a model load) | 7.0 | 5.8 | - |
| Llama 3.2 3B, slowest request | 14.4 s | 9.2 s | 18.0 s | - |

Requests wait their turn; none failed. The total speed does not grow past 2 at once on 1 vCPU.

Everything at once for 3 minutes (2 chat streams on Qwen3 0.6B, embeddings, Kokoro TTS, moonshine-tiny STT, a
Groq chat): 71 chat answers (average 5.0 s), 212 embedding calls (0.8 s), 16 TTS (11.3 s), 16 STT (11.3 s), no
local failure. All 97 Groq requests failed with Groq's free-tier limit (429 "Rate limit reached ... requests per
minute"); the server logged the reason for each.

#### Cloud provider, automation and the coding agent

| Test | Result |
|---|---|
| `cluaiz provider add groq --key-env GROQ_API_KEY`, `provider models` | works; the key stays in the environment |
| Coding agent with `groq/openai/gpt-oss-20b`: write `calc.py` and `test_calc.py`, run the tests | done in 4 s; our own `python3 -m unittest` run: OK. Cluaiz used no CPU worth noting while the provider answered |
| Coding agent with local Llama 3.2 3B | did not start: "Does not fit ... 0.89 GB short" (see problem 1) |
| Flow `vps-notes`: local model step, Groq step with a JSON schema, file write | `validate` and `save` work; `flow run` ended with "tasks.result ... error sending request" after 10 s and the file was not written (see problem 2) |
| Qwen3 8B (5.27 GB needed, 3.8 GB machine) | refused before loading, with the missing amount |

#### Problems found

1. **The CLI misses a busy server.** `agent` (and `run`) check for a running server with an 800 ms request
   ([`chat/remote.rs:25`](../../cmd/src/chat/remote.rs)). With the one vCPU busy, the check timed out, so
   the agent tried to load the model in its own process next to the server and the planner refused for memory.
2. **`cluaiz flow run` gives up on a busy server.** Each `tasks.result` request waits 5 s on the server and times
   out after 10 s on the client ([`cli/flow.rs:12`](../../cmd/src/cli/flow.rs)); here the model load took
   8 s and the CPU was full, the request failed and the command exited 1. The run's final state and why the file
   was not written are not known yet.
3. From run 18: Llama 3.2 3B left `<|python_tag|>` in the answer after a tool result; the coding agent reported
   success when a 3B model changed nothing.
4. From run 16: Ternary-Bonsai Q2_0 is 13-20% slower than official llama.cpp b12428 on this CPU.
5. Not Cluaiz: Groq's free tier allows a few requests per minute, so the provider part of the load test failed.
6. On a server reachable from a network, API authentication should be on (`cluaiz token create`); Cluaiz prints
   this note when a provider is added and authentication is off.

#### Sizing (from these measurements)

Everything ran on this one machine (1 vCPU, 3.8 GB); smaller machines were not tested. The memory column is what
Cluaiz held at the peak of that phase, plus about 0.6 GB for the operating system.

| Use | Cluaiz memory at the peak | Machine this points to |
|---|---|---|
| API server with cloud providers, flows, agent with a provider model | under 0.1 GB | 1 vCPU, 1 GB |
| Speech (Kokoro TTS, moonshine or whisper-base STT) and embeddings | 1.7 GB anon + mapped files | 1 vCPU, 2-4 GB |
| Chat models up to 1.7B (14-36 t/s generation) | up to 1.0 GB anon | 1 vCPU, 2 GB |
| Chat models of 3-4B (6-9 t/s) with speech | 2.0 GB anon | 1 vCPU, 4 GB |
| 7-8B chat models | needs 5.3 GB by the plan | 8 GB or more |

More vCPUs were not tested here; on 1 vCPU, parallel requests share the same speed.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-16-run19-vps-deep/`](../cloud/runs/2026-11-16-run19-vps-deep/)

<a id="run-6"></a>

## Run 6 (2026-11-09)

> **Summary.** Release v0.1.0 (first build, commit `e3beb91c`) installed by its one command on clean cloud
> machines: AWS Graviton2 (Linux ARM64, the first run of that core on hardware), an AWS Sapphire Rapids Xeon
> (Ubuntu 24.04 and Windows Server 2025), Docker images of Ubuntu 22.04 / 24.04, Debian 12 / 13, Rocky 9 and
> Fedora 42, and a GitHub macOS 15 runner (Apple M1, virtual). Every supported system installed, answered with
> Qwen3 0.6B and benchmarked; older Linux stopped with the glibc message. The runs found the bugs fixed in
> `f2769a6c`, `18edfb0c`, `2c9c76b9`, `c30bb908`, `772d790d` and `b3293e20`; v0.1.0 was then rebuilt from
> `bbaae441` with those fixes.

No GPU in this run: these machines have none (or, on macOS, a virtual one), so it covers install, the CPU
build, the planner and the server. Model: `unsloth/Qwen3-0.6B-GGUF` Q4_K_M, `cluaiz bench -r 2` (pp512, tg128).

#### Machines and results

| Machine | OS | Install | Model answers | pp512 t/s | tg128 t/s | CPU build ggml loaded |
|---|---|---|---|---:|---:|---|
| AWS t4g.small: Graviton2 (Neoverse-N1), 2 vCPU, 1.8 GB | Ubuntu 24.04 ARM64 | ✅ `install.sh` | ✅ | 56.6 | 23.9 | `armv8.2_2` |
| AWS c7i-flex.large: Xeon Platinum 8488C (AVX-512, AMX), 1 core / 2 threads, 3.7 GB | Ubuntu 24.04 x64 | ✅ `install.sh` | ✅ | 192.3 | 19.3 | `sapphirerapids` |
| AWS m7i-flex.large: same Xeon, 7.7 GB | Windows Server 2025 | ✅ `install.ps1` | ✅ | 130.9 | 18.5 | `cascadelake` (no sapphirerapids build on Windows) |
| GitHub `macos-15`: Apple M1 (virtual), 7 GB | macOS 15.8 | ✅ `install.sh` | ✅ | 176.7 | 2.1 | Metal (`MTL0`, virtual GPU) |
| Docker on the x64 machine | Debian 13, Fedora 42, Ubuntu 24.04 | ✅ | ✅ | | | |
| Docker on the x64 machine | Ubuntu 22.04 (glibc 2.35), Debian 12 (2.36), Rocky 9 (2.34) | ⛔ stops: "Cluaiz needs glibc 2.38 or newer" | | | | |

Also on the 3.7 GB x64 machine: Qwen3 4B Q4_K_M (2.3 GB file) planned CPU only, 4096 tokens, 2.91 GB, answered;
pp512 23.1 t/s, tg128 3.3 t/s.

The macOS runner's GPU is virtual: the run shows that auto placement uses Metal (`Full GPU`, all layers), not
how fast a real Mac is. Speeds on a real Mac: [M5 run](#mac-m5).

#### Checks

| Check | Result |
|---|---|
| PATH after install | bash: `~/.bashrc`; zsh with no `~/.zshrc` (a new macOS account): the file is created; fish: `conf.d/cluaiz.fish`. A new shell finds `cluaiz` in each (Ubuntu and macOS) |
| Windows PATH | the user PATH gets `~\.cluaiz\bin` |
| `cluaiz status` | names the GPU backend that loaded (`none, models run on the CPU`, or `MTL0: ...`) |
| Port already in use | `port 8011 is already in use by another program ... cluaiz permission port <number>`, exit 1 |
| `cluaiz serve` + `/health` | 200; stops on Ctrl+C |

#### Bugs found, and their fixes

| Bug | Fix |
|---|---|
| The README Quick Start `cluaiz run <repository> "prompt"` printed "already downloaded" and did not answer; a first `run` downloaded and stopped | `f2769a6c` |
| GitHub answered HTTP 500 for a release file for a few minutes after publishing, and the install failed | `18edfb0c` (curl retries 3 times) |
| `api.github.com` answered 403 (60 calls an hour per IP) on GitHub's macOS runners; the same happens on shared office, CI and mobile IPs | `2c9c76b9` (fall back to the releases feed) |
| The pull summary showed Qwen3-0.6B as "6B" | `c30bb908` |
| `status` named Windows Server 2025 "Windows 11" | `772d790d` |
| `cluaiz mcp start` and `cluaiz plugin link` only printed a message | `b3293e20` (removed) |
| The installer printed color codes into logs | `18edfb0c` |
| `cluaiz.com/install.sh` answered 403 (a Cloudflare challenge) to cloud IPs | the site's bot setting was changed; 200 from AWS since |

#### Not covered

GPU packs (CUDA 12 / 13 on Windows and Linux other than the RTX 3050 and the T4 above, Vulkan), integrated
GPUs, two GPUs, and speed on a real Mac.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-09-run6-cpu-os-matrix/`](../cloud/runs/2026-11-09-run6-cpu-os-matrix/)

<a id="mac-m5"></a>

## Mac: MacBook Air M5 16 GB (2026-10-28)

> **Summary.** The macOS release installed with its one command in about 2 minutes and served chat. At the same
> settings it matched plain llama.cpp (`llama-bench` built with Metal from the same llama.cpp commit): Qwen3-4B
> prompt 104–111 vs 105–110 t/s, generation 22.7–24.6 vs 22.1–22.5 t/s. But on this Mac the engine never used
> the Metal GPU: the hardware probe finds only NVIDIA GPUs, it read free RAM as 0.00–0.75 GB of 16 GB, and a
> request's `n_gpu_layers -1` was changed to 0. Auto placement refused even the 2.3 GB model (HTTP 500), and
> Gemma 3 12B was skipped. The numbers above are therefore the CPU with llama.cpp's op offload, not Metal.

#### Machine
| | |
|---|---|
| Mac | MacBook Air (Mac17,3), Apple M5: 4 performance + 6 efficiency cores, 8-core GPU, Metal 4 |
| RAM | 16 GB unified |
| OS | macOS 26.6.2 (25G83) |
| Power | charger connected |
| SSD | 2 GB written at 5.1 GB/s |
| Install | `install.sh` from `main`, release v0.1.0 core `mac-arm64` (77.6 MB, checksum OK), `CLUAIZ_HOME=~/cluaiz-mactest/home`, port 8011 |
| Script | [`test/mac/mac_test.sh`](../mac/mac_test.sh), 41 minutes (35 of them model downloads) |

The tester's user name, host name and the Mac's serial number and IDs were removed from these logs.

#### Results (Qwen3-4B Q4_K_M, tokens per second)

| Mode | Result | What the engine ran | Engine prompt / gen | llama-bench prompt / gen (same settings) |
|---|---|---|---|---|
| auto | **FAIL** (HTTP 500) | refused: "needs 2.91 GB more RAM than allowed" | - | - |
| gpu (`n_gpu_layers -1`) | PASS | `n_gpu_layers 0` | 104 / 22.7 | 110 / 22.1 |
| split | PASS | `n_gpu_layers 0` | 111 / 24.6 | 105 / 22.5 |
| cpu (`n_gpu_layers 0`) | PASS | `n_gpu_layers 0` | 88 / 17.2 | 82 / 16.3 |

Gemma 3 12B Q4_K_M (6.8 GB): skipped by `run.py`, because the engine reported less free RAM + VRAM than the file.
No run had every layer on the Metal GPU, so how far these numbers are from Metal speed is not known from this run;
`mac_test.sh` now also runs `llama-bench -ngl 99` on each model for that reference.

#### Engine problems found
| # | Problem | Evidence | Status |
|---|---|---|---|
| 1 | The hardware probe asks NVML only, so an Apple GPU (and an AMD or Intel GPU on Vulkan) is not seen: `GPU: none`, auto placement never offloads. llama.cpp itself loaded the Metal backend (`Backends loaded; GPU offload available`). | `planner.log`, `server.log` line 8 | fixed on `main`: GPUs of llama.cpp's own backends when NVML finds none; on unified memory the GPU part is counted in RAM too |
| 2 | Free RAM read as 0.00 GB (once 0.75) of 16 GB. sysinfo 0.30 on macOS counts free + inactive + purgeable pages minus the compressor's pages, which reaches zero when macOS compresses memory, so the planner refused a 2.9 GB plan. | `planner.log` `RAM: free 0.00 of 16.00 GB` | fixed on `main`: free RAM computed as Activity Monitor does |
| 3 | A request's `n_gpu_layers -1` became 0 when no GPU was detected, against the rule that a user setting goes to llama.cpp as it is. | `planner.log` `User setting: ... n_gpu_layers 0` | fixed on `main` (planner) |
| 4 | A refused load answers HTTP 500 (server error) instead of a client error with the reason. | `run_qwen3-4b-gguf.log` | fixed on `main`: HTTP 503 with `code: insufficient_memory` |
| 5 | Install output: `curl: (56) Failure writing output` while finding the release, and a "Ghost Execution Detected" warning for the binary the installer just placed. Neither stopped the install. | `install.log` | fixed on `main` |

The fixes are checked by unit tests and a macOS build check of the hardware crate; they have not run on a
Mac yet. That needs a release with them (`CLUAIZ_VERSION`) and `mac_test.sh` again.

#### Files
`mac_test.log` (the console), `machine.txt`, `install.log`, `installed_files.txt`, `pulls.log` and `pull_*.log`,
`llama_bench_build.log`, `server.log`, `planner.log` (every plan the server printed), `run_*.log` and
`RESULTS_*.md` (from `test/engine_regression/run.py`), `loaded_libs.txt` (empty: the server had already unloaded).

Logs, raw results and scripts: [`test/mac/runs/2026-10-28-m5-air-16gb/`](../mac/runs/2026-10-28-m5-air-16gb/)

## All logs

| Run | Folder |
|---|---|
| 72 | [`2026-11-25-run72-linux-release-soak`](../cloud/runs/2026-11-25-run72-linux-release-soak/) |
| 53 | [`2026-11-23-run53-api-matrix`](../cloud/runs/2026-11-23-run53-api-matrix/) |
| 19 | [`2026-11-16-run19-vps-deep`](../cloud/runs/2026-11-16-run19-vps-deep/) |
| Mac M5 | [`2026-10-28-m5-air-16gb`](../mac/runs/2026-10-28-m5-air-16gb/) |
| 6 | [`2026-11-09-run6-cpu-os-matrix`](../cloud/runs/2026-11-09-run6-cpu-os-matrix/) |
