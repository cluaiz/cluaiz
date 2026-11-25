# Run 72: Linux release build, end-to-end checks and a two-hour soak on the L4 VM (2026-11-24/25)

## Setup

| Item | Value |
|---|---|
| Machine | GCP g2-standard-8 (the run 48 VM): Intel Xeon 2.20 GHz, 8 vCPU, 31 GiB RAM, NVIDIA L4 23 GB, driver 620.70.06; Debian 13, kernel 6.12.117, glibc 2.41 (`raw/e2e/machine.txt`) |
| Build | `build.sh`: `cargo build -p cmd --release --locked` of `7498b6b4` (llama.cpp submodule `96b0cfb2d`), 1,473 s, rustc 1.100.0, CUDA toolkit 13.2 with `CUDA_ARCHITECTURES=89` (L4 only), then `.github/scripts/package_core.sh` and the CUDA driver + runtime libraries beside it, as the release packs install them. Not a CI artifact (CI builds the default arch set with the 12.8.1 / 13.4.1 containers). Binary SHA-256 `06fbf091...82012916` (`raw/build.log`, `raw/e2e/machine.txt`) |
| Server | `cluaiz serve` from the package folder, fresh isolated `CLUAIZ_HOME` (`~/run72/home`), `CLUAIZ_PORT=8013`, `CLUAIZ_HOST=127.0.0.1`; auth settings left at their defaults (not required, not changed) |
| Models | pulled with `cluaiz pull`: Qwen3.5-0.8B Q4_K_M (chat), embeddinggemma-300M Q8_0, Kokoro-82M UINT8, moonshine-tiny INT8 (`raw/e2e/pulls.txt`) |
| Tools in the home | math plugin, filesystem MCP (`cli_plugin_install`, `cli_mcp_install`) |
| Scripts | `e2e.sh` (CLI `cli_checks.sh`, HTTP `release_e2e.py`, `tools_checks.py`, the CEP and MCP journeys of `test/cep`), `e2e_node.sh` (second pass after Node.js 20 / npm 9 were installed, so the filesystem MCP could start), `soak.sh` (unchanged `test/release_soak/soak.py` + `bg_users.py` + `sampler.sh`), `analyze.py`, `scrub.sh` |
| Rerun | on the VM: `bash build.sh <repo> <out>`; `bash e2e.sh <package> <repo> <out>/e2e`; `bash e2e_node.sh <package> <out>/e2e`; `bash soak.sh <package> <repo> <out>/soak 120 7201`; `python3 analyze.py <out>/soak` |

## Results: end-to-end checks (`raw/e2e/`)

| Part | Result |
|---|---|
| CLI, 26 commands (`cli_summary.txt`) | 23 exit 0. `flow save` exit 1: non-interactive, the permission prompt read no answer and the flow was not saved (by design); `flow run` and `flow logs` then exit 1 (no such flow). With `y` answered: saved, `flow run` succeeded in 17,524 ms, 2 steps (`cli_flow_save_yes.txt`, `cli_flow_run_after_save.txt`). `run` answered `4`; headless `agent` read notes.txt and answered 4917; `bench` pp64 5,205 t/s, tg16 258 t/s (Full GPU, 1 run) |
| HTTP, 41 checks (`http_summary.txt`) | 40 / 41. The one failure is `chat_vision` text first: red 448x448 image, the model called `read_item` for "image" and did not name the colour; image first answered "solid red". Same bug as run 76 (fixed there in `0961d0ee`, after this build). `long_input_option_b` 3 / 3 real answers with the marker, 10.5-10.6 s (filesystem MCP not running in this pass) |
| HTTP second pass with Node.js (`http_summary_node.txt`) | 4 / 5: `chat_tools`, `vision_tools_call`, `json_schema_with_tools` pass; `long_input_option_b` 3 / 3 with the marker, 127.5-127.7 s (now with the filesystem MCP's tools in the loop); `chat_vision` text first fails as above |
| Tool and permission checks (`tools_summary.txt`, `tools_summary_node.txt`) | first pass 7 / 8 (`A2_files_tool_default_workspace`: listing null, because without `npx` the filesystem MCP could not start); second pass 8 / 8. The `..` listing and `../session.token` read did not leak the token; `workspace.set` to the home is refused; math runs without an approval prompt in sandboxed mode; a waiting flow step shows as running |
| CEP journeys (`journeys_cep.txt`) | 8 / 8 (116 methods in `rpc.discover`; loopback `tasks.list` p50 0.19 ms, p95 0.23 ms) |
| MCP journeys (`journeys_mcp.txt`) | 14 / 14 |

## Results: soak (`raw/soak/`, `raw/soak/analysis.md` from `analyze.py`)

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

## What this proves

- The release layout built from source on Debian 13 with CUDA runs every feature group on the L4: chat, stream,
  tools, JSON schema, thinking, vision with tools, embeddings, TTS, STT, Ollama routes, CEP, MCP, flows, CLI agent.
- Under two hours of mixed load with restarts, no crash outside planned restarts; failed requests only in restart
  windows, and two known bugs: the `max_tokens` budget (fixed later, run 78) and the text-first vision answer
  (fixed later, run 76).

## What this does not prove

- Not the CI release artifact (own build, L4 arch only, CUDA 13.2); no CPU-only or Vulkan pack; one model size.
- `long_input` under load: whether it would finish with a longer client timeout, and why the one reply was call
  markup, is not analysed here (runs 70 / 73 cover the loop's final answer without load).
- Memory over hours in one process (restarts every about 12 minutes).

## Files

`raw/build.log`; `raw/e2e/` (machine, pulls, CLI outputs, HTTP and tools JSONL + summaries, journeys, serve logs and
highlights); `raw/soak/` (soak.py's `checks.jsonl`, `events.jsonl`, `requests.jsonl`, `responses.jsonl`,
`memory.jsonl`, `engine.log.gz` (gzipped, 6.6 MB plain), `bg.jsonl`, `sampler.jsonl`, `analysis.md`, GPU snapshots).
Scrubbed on the VM with `scrub.sh` before copying (user name `<user>`, session token, VM addresses and cloud
project id: 0 files left with any of them); no token in any file.

## Pending

| Item | Needs |
|---|---|
| `long_input` under load with a longer client timeout | a soak rerun with the fixed build |
| Soak with the run 76 / 78 fixes | a new Linux soak (VM time) |
| CI release artifact on Linux | `install.sh` of the next pre-release on the VM |
