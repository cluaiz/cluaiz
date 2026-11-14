# Run 8: Linux, NVIDIA L4, release v0.1.0 (2026-11-14)

| | |
|---|---|
| VM | Google Cloud `g2-standard-8` (standard, not Spot), us-central1-c |
| CPU / RAM | Xeon 2.2 GHz, 8 vCPU / 31 GiB |
| GPU | NVIDIA L4 23 GB, driver 620.70.06 (Google's `cuda_installer.pyz`), no CUDA toolkit |
| OS | Debian 13 (trixie), kernel 6.12 |
| Release | v0.1.0 republished from `14ae569f` (Rust 1.100, ort rc.15 / ONNX Runtime 1.31) |
| Install | `install.sh` chose core + `llama-cuda13` + `cudart13` + `onnx-cuda13`, 43 s |
| Home / port | isolated `~/ch`, ports 8011 (release checks) and 8012 (features) |

Raw logs: `results/` (release checks, `results.json`, `server.log`, `loaded_libs.txt`), `t7/features/`
(CLI and CEP sweep, `summary.tsv`), `t7/extra/` (benches, speech files, flows, agent, `summary.tsv`).

## Release checks (`test/cloud/remote/release_tests.sh`, 3 mixed rounds)

25 of 25 calls passed.

| Call | Model | Placement / context | Prompt t/s | Gen t/s | Seconds |
|---|---|---|---|---|---|
| chat short | Qwen3 4B Q4_K_M | GPU only / 16384 | 933 | 83.5 | 3.0 |
| chat long | Qwen3 4B Q4_K_M | GPU only / 16384 | 4981 | 75.7 | 1.1 |
| chat short | Gemma 3 12B Q4_K_M | GPU only / 16384 | 127 | 31.2 | 5.8 |
| chat long | Gemma 3 12B Q4_K_M | GPU only / 16384 | 1784 | 28.2 | 2.3 |
| chat short | Mistral Small 3.2 24B Q4_K_M | GPU only / 16384 | 988 | 17.2 | 10.4 |
| chat long | Mistral Small 3.2 24B Q4_K_M | GPU only / 16384 | 1007 | 16.8 | 4.9 |
| TTS | Kokoro 82M UINT8 (ONNX) | | | | 3.6 (mixed 2.2–2.5) |
| STT | Whisper large-v3 turbo INT8 (ONNX) | | | | 11.1 (mixed 3.6); "Rivers carry water to the sea." exact |
| embedding | bge-m3 Q8_0 (GGUF) | | | | 1.8 (mixed 0.01) |
| TTS | Qwen3-TTS 1.7B Q4_K_M (GGUF) | | | | 3.6 (mixed 2.5–3.5) |

`loaded_libs.txt`: libcudart / libcublas 13 and libggml-cuda from `~/.cluaiz/bin` only.

## Bench (`cluaiz bench -r 2`)

| Model | pp512 t/s | tg128 t/s |
|---|---|---|
| Qwen3 4B Q4_K_M | 5128 ± 642 | 85.1 |
| Gemma 3 12B Q4_K_M | 1863 ± 90 | 31.4 |
| Mistral Small 3.2 24B Q4_K_M | 1023 ± 14 | 17.3 |
| Gemma 4 31B Q4_K_M (18.7 GB) | failed: "Context Init Failure; the retry with a smaller placement failed too" | |

`cluaiz run` of the same Gemma 4 31B answered ("Ganges, Yamuna, and Brahmaputra.").

## Features

| Area | Result |
|---|---|
| CLI local commands (`status`, `list`, `show` of every model, `show` by repository name, `config`, `token`, `permission`, `skill`/`plugin`/`mcp`/`provider list`) | pass |
| `run` (one prompt, think off) | pass |
| `bench` by repository name (`unsloth/Qwen3-4B-GGUF`) | pass (fix `14ae569f`) |
| `ps`, `storage temp-status`, `ingest` (Markdown) | pass |
| Flows: `nodes`, `validate`, `save` (permission confirmed), `run` (2 steps, 3 s), `logs`, `explain`, `remove` | pass |
| `flow draft` | not run on GPU (fails on the CPU run, see run 7) |
| Agent: list the files of a folder | pass, `list_dir` tool, 9 s |
| CEP `journeys.py` (discover, batch, tasks, audit, latency) | 8 of 8 |
| MCP `mcp_journey.py` | 14 of 14 |
| Speech files | `t7/extra/kokoro_en.wav`, `kokoro_hi.wav`, `qwen3tts_en.wav`; Whisper read the Qwen3-TTS file back as "ClueEyes runs language, speech and vision models on your own machine." |

## Findings

1. **ONNX ran on the CPU in the first session.** The server log says cuDNN 9 was not installed yet and
   is provisioned in the background; in the next session "CUDA provider registered" and ONNX used the GPU.
   The first speech calls after an install are therefore CPU speed.
2. **`bench` fails for Gemma 4 31B Q4_K_M** on 23 GB while `run` works: the bench's context and its
   smaller-placement retry both fail to initialise.
3. **wave_b journeys** (`test/cep/wave_b/*_http_journey.py`) do not run on Linux: their harness
   (`test/context-runtime/harness.py`) stops the server with Windows `taskkill`, and `journey_home.py`
   expects a seed home with `engine/config` (homes now keep `config/` at the root).
4. Whisper writes the brand name as "ClueEyes" / "Klaise"; the rest of each sentence is exact.
