# Run 16: the 1 vCPU VPS, Ternary-Bonsai with the pull fix, and Ollama with the same settings (2026-11-16)

| | |
|---|---|
| Machine | Same VPS as run 15: 1 vCPU (AMD EPYC 9354P, AVX-512), 3.8 GB RAM, no GPU, Ubuntu 24.04 |
| Cluaiz | v0.1.0 re-published release (tag `eb11d47d`), `cluaiz bench`; binary sha256 `bf85de0e…` |
| llama.cpp | official ggml-org prebuilt `b12428`, `llama-bench` |
| Ollama | official release 0.40.16 (`ollama-linux-amd64.tar.zst`), run standalone on its own port and model folder |
| Script | [`vps_run16_ollama.sh`](../../remote/vps_run16_ollama.sh) |

Settings: 1 thread, CPU only, prompt 512, generation 128, 3 runs. Ollama has no bench tool: `/api/generate`
with a raw prompt of 514 tokens and `num_predict` 128, one warm-up and 3 measured runs, t/s from Ollama's own
`prompt_eval` and `eval` timings; `num_thread` 1, `num_gpu` 0, `num_ctx` 4096, flash attention on.

Prompt / generation t/s. Cluaiz and llama.cpp rows for Bonsai 4B and Llama 3.2 3B are from run 15 (same
machine, same day).

| Model | Mode, KV | Cluaiz | llama.cpp | Ollama |
|---|---|---|---|---|
| Bonsai 4B Q1_0 | cpu, F16 | 7.82 / 6.49 | 7.77 / 6.56 | 7.27 / 4.99 |
| Bonsai 4B Q1_0 | auto, Q8_0 | 6.75 / 5.79 | 6.71 / 5.99 | 6.64 / 4.50 |
| Llama 3.2 3B Q4_K_M | cpu, F16 | 21.17 / 8.16 | 21.80 / 7.60 | did not load (see below) |
| Llama 3.2 3B Q4_K_M | auto, Q8_0 | 17.10 / 7.86 | 16.71 / 7.73 | 18.84 / 6.74 |
| Ternary-Bonsai-2-4B Q2_0 | cpu, F16 | 5.51 / 4.25 | 6.54 / 4.86 | 5.77 / 4.32 |
| Ternary-Bonsai-2-4B Q2_0 | auto | 5.37 / 4.23 (F16) | 6.75 / 5.04 (F16) | 5.82 / 4.11 (Q8_0) |

## Findings

- The pull fix works on the release: `:Q2_0` now takes `Ternary-Bonsai-2-4B-Q2_0.gguf` (run 15 took PQ2_0).
- Ternary-Bonsai-2-4B Q2_0: Cluaiz is 13-20% slower than official llama.cpp b12428 (prompt and generation),
  and about equal to Ollama. On the same machine Q1_0 and Q4_K_M are equal to llama.cpp, so this looks
  specific to the Q2_0 CPU code in the llama.cpp version Cluaiz bundles; not checked yet.
- Ollama generates 14-23% slower than Cluaiz on Bonsai 4B and Llama 3.2 3B (about equal on Ternary-Bonsai);
  its prompt speed is within -7% to +10% of Cluaiz.
- Ollama could not load Llama 3.2 3B in the F16 run: its llama-server was killed by the kernel for memory
  (`signal: killed`, 1.6 GB free) because the previous model was still loaded and Ollama loads weights into
  RAM (`--load-mode none` on the CPU). Cluaiz memory-maps the file and ran it. The website container on this
  VPS kept running.
- In auto, Cluaiz's planner chose F16 KV for Ternary-Bonsai and Q8_0 for the other two; the Ollama auto runs
  used Q8_0 for all three.
