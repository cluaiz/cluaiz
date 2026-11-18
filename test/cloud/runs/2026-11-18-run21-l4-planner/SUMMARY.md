# Run 21: the planner on an L4, 6 release models and 16 architectures (2026-11-18)

| | |
|---|---|
| Machine | Google Cloud `g2-standard-8` (standard, us-central1-c): NVIDIA L4 23,034 MiB, 8 vCPU, 31 GB RAM, Ubuntu, CUDA 13.2 |
| Cluaiz | `main` at `7f489c06`, built with `remote/linux_build.sh - 89`. Runs 21b and 21c add the two fixes of this run (below) on top |
| Scripts | [`remote/run21_planner.sh`](../../remote/run21_planner.sh), [`remote/vram_hog.py`](../../remote/vram_hog.py), [`remote/multiturn_cache.py`](../../remote/multiturn_cache.py); 21c: [`run21c.sh`](run21c.sh) |
| Records | [`results-run21/`](results-run21/) (21 and 21b), [`results-run21c/`](results-run21c/), [`results-run21-first/`](results-run21-first/) (outputs before the fixes), `run21*.log` |
| Home | isolated `CLUAIZ_HOME` per run (`~/ch-run21`, `~/ch-run21c`), port 8011 |

The first attempt ran on a Spot VM (`cluaiz-l4b`). Google reclaimed it twice, and the run moved to the
standard VM. Qwen3-30B-A3B and gpt-oss-20b were linked from an earlier run's home, not pulled again.

## Results: VRAM planned against taken

"Planned" is the planner's corrected estimate. A buffer is kept free beside it: 0.15 GB reserve + 0.30 GB
for the GPU pool for a model not measured yet, from the log line `VRAM planned X, taken Y`.

### Release models (`bench --placements -p 512 -n 128`)

| Model | Placement | Planned | Taken | pp512 t/s | tg128 t/s |
|---|---|---|---|---|---|
| Llama 3.2 3B Q4_K_M | Full GPU | 3.90 | 3.94 | 7,593 | 109.6 |
| Gemma 3 12B Q4_K_M, **before the fix** | Full GPU | 12.93 | 8.85 | 2,014 | 31.7 |
| Gemma 3 12B, **after both fixes** (21c, fresh home) | Full GPU | 8.81 | 8.85 | 2,042 | 31.7 |
| Mistral Small 3.2 24B Q4_K_M | Full GPU | 15.79 | 15.84 | 1,066 | 17.5 |
| Qwen3-30B-A3B Q4_K_M (MoE) | Full GPU | 18.93 | 18.98 | 3,275 | 102.8 |
| gpt-oss-20b MXFP4 | Full GPU | 11.50 | 11.55 | 3,614 | 83.1 |
| Gemma 4 31B Q4_K_M | Full GPU | 20.04 | 20.06 | 765 | 13.0 |

Gemma 4 31B gets 16,384 tokens of context with all layers on the GPU (KV F16 2.42 GB, `show_gemma-4-31b.txt`).
Before the sliding-window load fix of `f52044c5` it got 4,096.

### Architecture sweep (`bench -p 128 -n 32 -r 1`, one load each, Q4_K_M)

| Architecture | Model | Planned | Taken | Taken - planned |
|---|---|---|---|---|
| llama | Llama 3.2 1B | 1.51 | 1.55 | +0.04 |
| gemma2 (sliding, default pattern) | Gemma 2 2B | 2.74 | 2.77 | +0.03 |
| gemma3 (sliding, default pattern) | Gemma 3 1B | 1.35 | 1.39 | +0.04 |
| gemma3n (shared KV) | Gemma 3n E2B | 2.09 | 2.12 | +0.03 |
| qwen3 | Qwen3 1.7B | 3.09 | 3.13 | +0.04 |
| qwen35 (hybrid) | Qwen3.5 2B | 1.88 | 1.93 | +0.05 |
| phi3 | Phi-4-mini | 4.72 | 4.77 | +0.05 |
| mistral3 | Ministral 3 3B | 3.90 | 3.94 | +0.04 |
| olmo2 | OLMo 2 1B | 1.47 | 1.51 | +0.04 |
| granitehybrid (Mamba hybrid) | Granite 4.0-H-Tiny | 4.32 | 4.41 | +0.09 |
| lfm2 (hybrid) | LFM2 1.2B | 1.07 | 1.12 | +0.05 |
| deepseek2 (MLA, MoE) | DeepSeek-Coder-V2-Lite | 14.02 | 14.47 | **+0.45** |
| deepseek2 (MLA, vision) | Kimi-VL-A3B-Thinking | 10.81 | 10.53 | -0.28 |
| deepseek2 (MLA) | Moonlight-16B-A3B | 10.36 | 10.26 | -0.10 |
| GLM-4.7-Flash | GLM-4.7-Flash | 18.66 | 18.08 | -0.58 |
| kimi-linear (hybrid, MoE split) | Kimi-Linear-48B-A3B | 21.08 | 21.82 | **+0.74** |

All 16 loaded and ran; none failed. 14 are within 0.09 GB of the plan.

Two are under the plan:
- **DeepSeek-Coder-V2-Lite:** +0.45 GB, exactly the buffer kept for an unmeasured model.
- **Kimi-Linear 48B:** +0.74 GB, more than that buffer. It loaded because the GPU was otherwise empty.

The pool gap learned from such a first load covers the next load. These two layouts (MLA, Kimi-Linear's
linear-attention state) are the next estimate work. The MLA models of the Kimi and GLM families are
over-estimated, which is safe but wastes up to 0.58 GB.

## Bugs found and fixed in this run

1. **Sliding-window layers counted as full attention when the file lists none.**
   - Gemma 3 files carry `sliding_window` but no `sliding_window_pattern` array.
   - llama.cpp applies a per-architecture period (`load_swa_pattern(ml, 6)` in `src/models/gemma3.cpp`; 2 for gemma2 and gpt-oss, 4, 5 or 8 for others).
   - The prober now does the same (`engine/models/src/prober/gguf.rs`, `default_swa_pattern`, `swa_pattern`, with a unit test).
   - Gemma 3 12B KV at 16,384 tokens: 6.00 GB before, the llama.cpp layout after.
2. **Learned margins measured against the old estimate were applied to the new one.**
   - A pool gap of -3.89 GB, learned while the estimate was 4 GB too high, made the first load after the fix under-plan by 0.64 GB (21b: planned 8.39, taken 9.03).
   - The correction and pool-gap keys now carry a version (`proj3:`, `pool2:` in `engine/plan/src/margin.rs`), so older ones are dropped, as the code already did for `proj2:`.
   - 21c in a fresh home: planned 8.81, taken 8.85.

Unit tests after both fixes: `cluaiz-models` 43 passed, `cluaiz-plan` 54 passed (release).

## Step-down when another process takes VRAM during a load

- **21:** invalid. The other process took the VRAM 6 s after the start and gave it back after 60 s, but
  reading Qwen3-30B-A3B from the disk took 104 s, so the load never saw it.
- **21c:** `vram_hog.py load 1.5 180` waits until the load has taken 1 GB, then takes all but 1.5 GB of
  what is free.
  - Log: `17:16:41 free 4.60 GB, took 3.10 GB`.
  - The planned full-GPU load failed ("Context Init Failure").
  - The Engine retried one placement smaller: MoE split, experts of 48/48 layers in RAM. That load
    worked and the bench ran (pp64 33.0, tg16 12.0 t/s). No crash.
  - The retry line names the same `-1 GPU layers, 16384 tokens`; the change is in `n_cpu_moe`
    (0 -> 48), which the message does not print.

## Multi-turn chat without a session id (Gemma 3 12B, `serve`)

| Turn | Answer | Prompt tokens | Reused from the cache | Processed |
|---|---|---|---|---|
| 1 | OK | 1,828 | 0 | 1,827 |
| 2 | BLUE-42 | 1,854 | 1,828 | 22 |
| 3 | EULB | 1,875 | 1,854 | 18 |

PASS: later turns reuse the whole earlier prompt, and the fact from turn 1 is kept.

## Not done

- No A/B against official llama.cpp on the L4 (done on the laptop on 2026-11-18, GPU parity).
- The planner estimate for MLA and Kimi-Linear is not fixed yet (see the sweep).
- The retry message does not print the changed `n_cpu_moe`.
