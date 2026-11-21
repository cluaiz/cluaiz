# Run 26: Cluaiz, llama.cpp and Ollama on a 1 vCPU VPS next to a running web app (2026-11-20/21)

Run by Gemini; this summary was rewritten from the raw files in this folder. An earlier summary said Cluaiz
was 2.6x faster. That was wrong: the host capped the CPU partway through, after Cluaiz's native stage
(section "The CPU cap"). Rechecks of the two Cluaiz bugs on the current code: [run 38](../2026-11-21-run38-run26-recheck/SUMMARY.md).

| | |
|---|---|
| Machine | VPS, 1 vCPU (AMD EPYC 9354P), 3,915 MB RAM, no swap, no GPU, Ubuntu 24.04 (`*/manifest.json`) |
| Neighbour app | a small web app with PostgreSQL, checked once a second in every phase (`remote/vps26/neighbour_app.py`) |
| Engines | Cluaiz 0.1.0 pre-release, a build from before `fe21984c` (sha256 in the manifest, commit not recorded); official llama.cpp `b12428`; Ollama 0.40.16. Native under `systemd-run` (memory limit 3,515 MB), and Docker |
| Settings, all engines | 1 thread, context 4096, Q8_0 KV cache, one slot, temperature 0 |
| Models | Qwen3 0.6B, Qwen3 1.7B, Llama 3.2 3B (Q4_K_M); EmbeddingGemma 300M Q8_0; Kokoro TTS and Moonshine STT (Cluaiz only) |
| Load | 5-minute steps at 1, 2, 4, 8, 16, 32 requests at once; then 1-hour soaks |
| Scripts | `test/cloud/remote/vps26/`; per-stage tables in each folder's `analysis.md`; large engine logs are gzipped |

Folders: `smoke/` (first short pass), `native_*`, `docker_*` (ramps), `soak_*` (1 hour each). Stage 10
(llama.cpp in Docker) was stopped after its Qwen3 0.6B steps; those steps are only in `master.log`.

## The CPU cap

The provider's panel showed the VPS at 100% CPU for about four hours, then a 40% cap for the rest of the
run. llama.cpp prints its own speed per request, in both llama-server and Cluaiz logs, so the cap shows in
the data. Qwen3 0.6B generation, median:

| Stage (start, UTC) | Engine | Generation t/s |
|---|---|---:|
| smoke (11-20 14:46) | llama.cpp / Ollama / Cluaiz | 35.8 / 35.3 / 36.3 |
| native (11-20 16:43) | Cluaiz | 35.9 |
| native (11-20 20:17) | Ollama | 13.9 (its first step at 20:22 matched Cluaiz: 0.651 s against 0.645 s per request; from 20:27 on it was capped) |
| native (11-20 23:50) | llama.cpp | 12.2 |
| Docker (11-21 07:28) | Cluaiz | 12.7 |

So the native ramps compare Cluaiz at full CPU with llama.cpp and Ollama under the cap; their speed and
"break" levels are not comparable. Everything from 11-20 about 20:25 UTC on ran under the same cap.

## Comparable results (all under the cap)

**1-hour chat soak, Qwen3 1.7B, ticket labels as JSON:**

| Engine | Requests in flight | OK | Machine memory peak | Engine anonymous memory peak | Neighbour app failures |
|---|---:|---|---:|---:|---|
| Cluaiz | 32 | 1,048 / 1,048 | 1,759 MB | 1,139 MB | 1 of 3,560 checks |
| llama.cpp | 16 | 1,073 / 1,073 | 1,811 MB | 1,147 MB | 0 of 3,509 |
| Ollama | 16 | 1,102 / 1,102 | 2,055 MB | 1,554 MB | 0 of 3,490 |

Generation speed in the three soaks: 6.0, 5.6 and 5.7 t/s. Throughput was level; Cluaiz held twice the
queue with the lowest machine memory. No OOM kill in any soak.

**Docker ramps, W1 (same cap):** seconds per request at one request at a time: Qwen3 0.6B Cluaiz 1.89,
Ollama 1.61; Qwen3 1.7B Cluaiz 3.68, Ollama 3.78; Llama 3.2 3B Cluaiz 6.44, Ollama 6.50. With 1.7B, the
95th-percentile wait passed 60 s at 16 requests for Cluaiz and at 32 for Ollama; with 3B at 16 for both.
No engine returned errors in these chat steps; "break" here means the wait passed 60 s.

## Cluaiz alone: mixed load for 1 hour

Chat (Qwen3 0.6B), embeddings, TTS and STT together, 16 requests in flight: 1,063 / 1,063 OK (698 chat,
204 embeddings, 81 TTS, 80 STT). Machine memory peak 2,434 MB of 3,915; the neighbour app passed all
3,492 checks. Median time per request under the cap and the queue: chat 83.8 s, embeddings 17.6 s,
TTS 39.9 s, STT 3.1 s. llama.cpp and Ollama have no speech route, so there is no comparison row.

## Two Cluaiz bugs in this build

1. **Embeddings refused a second request:** at 2 at once, 1,179 of 1,252 (native) and 994 of 1,021
   (Docker) got HTTP 429 `"Another GGUF embedding request is still active"`. Fixed in `fe21984c`
   (requests wait in line for up to 120 s); run 38 checked 2 and 4 at once on the current code.
2. **No usage chunk in streamed answers:** `in_tokens` is empty for every Cluaiz row, so the harness counted
   streamed chunks as tokens (Cluaiz 6-10 per answer, the others 14-15 tokens). Cluaiz's `Agg t/s` in the
   tables is a chunk rate. Run 38 showed the current code sends the usage chunk.

## Not proven

- Speed at full CPU between engines in this run (only the smoke pass and Cluaiz's native stage had it; run
  15 on this VPS, same settings: generation equal).
- The VPS rerun with a build at or after `fe21984c`.
- Answer texts were not saved, so answer lengths cannot be compared beyond the counts above.
