# Run 7: Linux CPU, release v0.1.0 (2026-11-14)

| | |
|---|---|
| VM | Google Cloud `n2-standard-8` (Spot), us-central1-a, no GPU |
| CPU / RAM | Xeon 2.8 GHz, 8 vCPU / 31 GiB |
| OS | Ubuntu 24.04.5 |
| Release | v0.1.0 republished from `14ae569f` (Rust 1.100, ort rc.15 / ONNX Runtime 1.31) |
| Install | `install.sh`, CPU core pack only |
| Home / port | isolated `~/ch` (models from run 3 reused), ports 8011 and 8012 |

Raw logs: `results/`, `t7/features/` (`summary.tsv`, `summary2.tsv`), `t7/extra/` (`summary.tsv`).

## Release checks (`test/cloud/remote/release_tests.sh`, 3 mixed rounds)

25 of 25 calls passed.

| Call | Model | Prompt t/s | Gen t/s | Seconds |
|---|---|---|---|---|
| chat short / long | Qwen3 4B Q4_K_M | 46.0 / 51.1 | 13.4 / 7.3 | 22.3 / 45.2 |
| chat short / long | Gemma 3 12B Q4_K_M | 10.6 / 19.0 | 4.4 / 3.4 | 61.4 / 118.4 |
| chat short / long | Mistral Small 3.2 24B Q4_K_M | 10.4 / 10.2 | 2.6 / 2.2 | 175.7 / 229.3 |
| TTS | Kokoro 82M INT8 | | | 4.8 (mixed 2.5–3.7) |
| STT | Whisper large-v3 turbo INT8 | | | 15.8 (mixed 2.8–4.6), exact |
| embedding | bge-m3 Q8_0 | | | 4.5 (mixed 0.05) |
| TTS | Qwen3-TTS 1.7B Q4_K_M | | | 23.1 (mixed 13.4–13.7) |

Run 3 (2026-10-27, same machine type) had Qwen3 4B at 10.5 gen t/s.

## Bench (`cluaiz bench -r 2`, CPU only, context 4096)

| Model | pp512 t/s | tg128 t/s |
|---|---|---|
| Qwen3 4B Q4_K_M | 60.1 | 14.0 |
| Gemma 3 12B Q4_K_M | 20.8 | 4.9 |
| Mistral Small 3.2 24B Q4_K_M | 10.7 | 2.8 |

## Features

| Area | Result |
|---|---|
| CLI local commands, `show` of all 8 installed models, `show`/`bench` by repository name | pass |
| `show` of a model not installed | clear error, exit 1 (expected) |
| `run`, `run --think off` ("51" for 17 × 3) | pass |
| `ps`, `storage temp-status`, `ingest` (Markdown: 1 chunk; asks for a vector model to embed) | pass |
| Flows: `nodes`, `validate`, `save`, `run` (2 steps, 13.6–19 s), `logs`, `explain`, `remove` | pass |
| `flow draft` | **fail**: "request (4806 tokens) exceeds the available context size (4096 tokens)" |
| Agent (default chat model set): list the files of a folder | pass, `list_dir`, 144–163 s |
| Agent without a default chat model | clear error naming the installed models |
| CEP `journeys.py` | 8 of 8 (p95 0.23 ms for `tasks.list`) |
| MCP `mcp_journey.py` | 14 of 14 |
| Speech files | `t7/extra/kokoro_en.wav`, `kokoro_hi.wav`, `qwen3tts_en.wav` |

## Findings

1. **`flow draft` cannot work at the CPU default context (4096):** its prompt is 4806 tokens.
2. **`flow save` has no `--yes`**; without a terminal the permission question is answered "no".
   Piping `y` works.
3. The wave_b journeys do not run on Linux (see run 8, finding 3).
