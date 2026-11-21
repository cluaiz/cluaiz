# Run 26: Cluaiz, llama.cpp and Ollama on a 1 vCPU VPS next to a running app (smoke pass, 2026-11-20)

Run by Gemini on the run 15 VPS; this summary was written afterwards from the raw files in `smoke/`.
Rechecks of the two Cluaiz problems on the current code: [run 38](../2026-11-21-run38-run26-recheck/SUMMARY.md).

| | |
|---|---|
| Machine | VPS, 1 vCPU (AMD EPYC 9354P), 3.8 GB RAM, no swap, no GPU, Ubuntu 24.04 (`smoke/*/manifest.json`) |
| Neighbour app | a small web app with PostgreSQL, called once a second during every phase (`remote/vps26/neighbour_app.py`) |
| Engines | Cluaiz 0.1.0 pre-release, a build from before 2026-11-20 23:30 (sha256 in the manifest, commit not recorded); official llama.cpp `b12428`; Ollama 0.40.16 |
| Settings, all engines | 1 thread, context 4096, Q8_0 KV cache, one slot, temperature 0 |
| Model | Qwen3-0.6B Q4_K_M for chat; EmbeddingGemma 300M Q8_0 for embeddings; Kokoro and Moonshine for speech (Cluaiz only) |
| Load | 25 s per step, 1 then 2 requests at once; native and Docker |
| Scripts | `test/cloud/remote/vps26/` (`run26.sh`, `workloads.py`, `analyze.py`); tables: `smoke/native/analysis.md`, `smoke/docker/analysis.md` |

Workloads: W1 support-ticket labels as JSON, W4 embeddings and search hit rate, W5 speech (TTS and STT),
W6 chat, embeddings and speech together.

## What the data shows

**Memory, chat (W1, native, peak of the whole machine with the app running):** Cluaiz 1,160 MB, llama.cpp
1,167 MB, Ollama 1,359 MB. The engine's own anonymous memory: Cluaiz 560 MB, llama.cpp 589 MB, Ollama
767 MB. Cluaiz and llama.cpp are level; Ollama used about 200 MB more.

**Memory, embeddings (W4 index):** machine peak Cluaiz 692 MB, llama.cpp 649 MB, Ollama 981 MB.

**Speech and everything together (Cluaiz only):** W5 machine peak 1,832 MB; W6 2,021 MB of 3,915 MB. The
neighbour app had no errors in any phase and no OOM kill was logged.

**Answers:** W1 JSON was valid in every answer for all three engines; label accuracy 0.32 to 0.37 for all
three (this is the 0.6B model, not the engine). Search top-1 hit rate: Cluaiz 0.875, llama.cpp 0.875,
Ollama 0.75.

**Time per W1 request, 1 at a time (median):** Cluaiz 0.63 s (first token 0.34 s), llama.cpp 0.72 s
(0.41 s), Ollama 0.70 s (0.40 s). With 2 at once: Cluaiz 1.28 s, llama.cpp 1.12 s, Ollama 1.14 s; in the
25 s step Cluaiz answered 39 requests, llama.cpp 47, Ollama 46.

## Two Cluaiz problems

1. **Embeddings with 2 requests at once failed.** 8,160 of 8,163 requests got HTTP 429
   `"Another GGUF embedding request is still active"`: the build refused a second request instead of
   waiting. Fixed in `fe21984c` (2026-11-20 23:30, after this run): requests wait in line for up to 120 s.
   Run 38 checked it on the current code (2 and 4 at once, all HTTP 200).
2. **Tokens per second for Cluaiz are not comparable in this run.** Cluaiz's streamed answers had no usage
   chunk (`in_tokens` is empty for every Cluaiz row), so the harness counted SSE chunks: 10 per answer,
   against 15 counted tokens from llama.cpp and Ollama. The tables' `Agg t/s` and `Req t/s` for Cluaiz are
   chunk rates, not token rates. The report image `vps_comparison_corrected.png` shows 52.1 t/s for Cluaiz
   by assuming 15 tokens; the answer texts were not saved, so that number is not proven either. The fair
   speed comparison on this machine is run 15 (same settings, `bench`): prompt and generation equal within
   the run-to-run spread. Run 38 showed that the current code sends the usage chunk and that its
   `completion_tokens` matches the content chunks.

## Not proven

- The commit of the Cluaiz build; the "slower" and "faster" readings of Cluaiz in the images.
- Long load: steps were 25 s and only 1 and 2 requests at once (a smoke pass, not the planned soak).
- Docker image sizes in the report image compare a CPU-only Cluaiz image built for this run (123 MB) with
  the full Ollama image (8.52 GB, with GPU libraries); they are not the same kind of image.
- W5 and W6 have no llama.cpp or Ollama rows: neither has a speech route.
