# Run 13: the MoE GPU cache of official llama.cpp on the L4 (2026-11-15)

Question: does official ggml-org llama.cpp's MoE cache (`--moe-cache-mib`, `--moe-cache-cpu-share`;
#29887, #30112, #30966, `docs/moe-cache.md`) make generation faster when the experts of a MoE model are
not in VRAM, and should the Cluaiz planner use it? Nothing in Cluaiz was changed for this run.

- Build: official llama.cpp `b54752d` (2026-11-15), `llama-server`, CUDA arch 89. Not the Cluaiz fork.
- Machine: the run 11 L4 VM (22.4 GB VRAM free, 31 GB RAM, 4 threads), the run 11 GGUF files.
- Server arguments in every case: `-c 8192 -t 4 -np 1`.

## Run 13a: sizes and shares chosen by hand

Script: [`moe_cache_test.sh`](../../remote/moe_cache_test.sh). 1 warm-up and 3 requests of 128 new
tokens; the generation t/s is the average of the 3. Experts are mmap'd (the default).

| Case | base | `GGML_OP_OFFLOAD_MIN_BATCH=1` | small cache, share 0 | small cache, share 0.5 | 8 GB cache |
|---|---|---|---|---|---|
| Qwen3-30B-A3B Q4_K_M, `-ngl 99 -cmoe` | 18.66 | 3.84 | 9.13 (2 GB) | 15.59 (2 GB) | 67.28 |
| gpt-oss-20b MXFP4, `-ngl 99 -cmoe` | 13.05 | 3.35 | 10.77 (2 GB) | 13.96 (2 GB) | 59.84 |
| gpt-oss-120b MXFP4 (59 GiB > RAM), `-ngl 99 -ncmoe 25` | 10.15 | 2.72 | 4.02 (1 GB) | 4.76 (1 GB) | – |

`GGML_OP_OFFLOAD_MIN_BATCH=1` (the pre-October Cluaiz setting, which uploads whole layers for every token) is
3.7–4.9x slower than the default in all three cases.

This part did not follow `docs/moe-cache.md`: the shares 0.9–0.98 were not tried, the experts were not
pinned, 128 tokens do not warm the cache, and the 120B cache was 1.7% of its experts (the PR suggests ~10%).
Its conclusion that a small cache is slower than none is therefore not valid; run 13b replaces it.

## Run 13b: llama.cpp's own tuner

Script: [`moe_cache_tune_run.sh`](../../remote/moe_cache_tune_run.sh), which runs `scripts/moe-cache-tune.py`.
That is 6 server runs per case: 2 cache sizes (10% and 30% of the experts, capped by free VRAM) × shares
0, 0.5 and 0.95. It also fits the machine's upload and CPU speeds. The pinned cases use `-lm none`.
Results: [`results-run13b/`](results-run13b/).

Measured generation t/s (the "base" column is run 13a):

| Case | base | ~10% cache, share 0 | ~10%, share 0.95 | ~30% cache, best share | Upload GiB/s | CPU GiB/s |
|---|---|---|---|---|---|---|
| Qwen3-30B `-cmoe`, mmap | 18.66 | 9.5 (1.6 GB) | **29.2** | 48.4 (4.9 GB, 0.5) | 4.1 | 25.3 |
| Qwen3-30B `-cmoe`, pinned | 18.66 | 21.2 | **34.0** | **65.7** (4.9 GB, 0) | 11.9 | 29.3 |
| gpt-oss-20b `-cmoe`, mmap | 13.05 | 5.2 (0.96 GB) | **15.3** | 26.2 (2.9 GB, 0.95) | 4.2 | 17.6 |
| gpt-oss-20b `-cmoe`, pinned | 13.05 | 12.1 | **16.2** | 28.9 (2.9 GB, 0.5) | 11.2 | 16.6 |
| gpt-oss-120b `-cmoe`, mmap (from the SSD) | 3.2* | 6.1 (5.8 GB) | 3.1 | 7.4 (17.4 GB, 0.95) | 2.3 | 3.8 |

\* gpt-oss-120b with `-ngl 99 -cmoe` and no cache: 3.64, 2.35 and 3.67 t/s (512 tokens each).
With the same file, `-ncmoe 25` and no cache gives 10.15 t/s (run 13a).

Cache hit rates at the 10% size: Qwen3-30B 59–63%, gpt-oss-20b 36–39%, gpt-oss-120b 55–86%.

Tuner recommendations (lowest share within 5% of the fastest, predicted with a warm cache, mmap):

| Cache / experts | Qwen3-30B | gpt-oss-20b |
|---|---|---|
| 5% | 0.98 | 0.98 |
| 10% | 0.98 | 0.95 |
| 20% | 0.95 | 0.95 |
| 30% | 0.9 | 0.9 |
| 40% | 0.8 | 0.9 |
| 50% | 0 | 0.8 |
| all experts | 0 | 0 |

With pinned experts the recommended shares are lower: Qwen3-30B goes from 0.95 at 5% to 0.5 at 30%, and 0 from 40%.

The time model of the tuner matched the runs within 0.1–1.2% for Qwen3-30B mmap and both gpt-oss-20b cases.
It matched only within 12.4% for Qwen3-30B pinned and 24.1% for gpt-oss-120b, so the tuner stopped with
exit 1. The measured numbers of those two cases stand; their predicted tables are weak.

## Findings

1. When the model fits in RAM and its experts are not in VRAM, the cache with a high CPU share is faster than no
   cache even at ~1 GB: 1.17–1.82x at ~10% of the experts and 2.0–3.5x at ~30%.
2. The share must follow the cache size: 0.9–0.98 for a small cache, 0–0.5 for a large one. With share 0, a small
   cache is slower than no cache, as run 13a measured.
3. Pinned experts (`-lm none`) upload at ~11–12 GiB/s instead of ~4 GiB/s and are faster, but the whole model is
   held in RAM.
4. When the model is larger than RAM, disk reads limit generation. The best cache setting of gpt-oss-120b
   (7.4 t/s) is slower than keeping the experts of 11 layers in VRAM with `-ncmoe 25` (10.15 t/s). There the
   planner's current SSD placement stays the better choice.

## Run 13c: a GPU smaller than the model, `--fit` against the cache

Script: [`moe_cache_fit_test.sh`](../../remote/moe_cache_fit_test.sh). Results: [`results-run13c/`](results-run13c/).

`--fit-target` makes the L4 act as an 8 GB or a 4 GB GPU. Each model, budget and load mode ran three settings:

- **fit:** llama.cpp's `--fit` puts as many expert layers in VRAM as fit, with no cache.
- **fit+cache:** the same, plus a cache of 10% of the experts. `--fit` counts the cache and keeps fewer layers.
- **cmoe+cache:** `-cmoe` with a cache in what the budget leaves after attention and KV.

The CPU share came from the run 13b rule: 0.95 for a cache of up to 12% of the experts, 0.9 up to 25%, 0.8 up
to 45%, otherwise 0, and one step lower with locked experts. Locked means `-lm mlock`, which pins the
experts. Each setting had 1 warm-up and 3 requests of 512 new tokens; the table gives the average.

| Model | Budget | Load | fit | fit+cache | cmoe+cache | Best vs fit |
|---|---|---|---|---|---|---|
| Qwen3-30B | 8 GB | mmap | 26.67 | 42.12 | **57.38** | 2.15x |
| Qwen3-30B | 8 GB | mlock | 26.69 | 47.74 | **65.45** | 2.45x |
| Qwen3-30B | 4 GB | mmap | 20.83 | **31.84** | 26.77 | 1.53x |
| Qwen3-30B | 4 GB | mlock | 20.97 | 33.60 | **34.78** | 1.66x |
| gpt-oss-20b | 8 GB | mmap | 27.93 | 33.77 | **37.17** | 1.33x |
| gpt-oss-20b | 8 GB | mlock | 27.95 | 35.62 | **52.89** | 1.89x |
| gpt-oss-20b | 4 GB | mmap | 15.45 | **17.36** | 16.77 | 1.12x |
| gpt-oss-20b | 4 GB | mlock | 15.53 | 17.96 | **21.57** | 1.39x |

GPU memory used stayed at the budget: 7.6–8.2 GB at 8 GB and 3.5–4.2 GB at 4 GB, as `nvidia-smi` read it.
The cmoe+cache setting used the least.

The cache settings varied more between requests than fit did. For example, gpt-oss-20b at 8 GB with mmap gave
34.2–42.5 t/s with cmoe+cache, against 27.90–27.95 t/s with fit.

Every cache setting was faster than `--fit` without a cache. At 8 GB, `-cmoe` with the cache won in all four rows.
At 4 GB with mmap, `--fit` with a 10% cache won narrowly; with mlock, cmoe+cache won.

## Not measured

- A real 4 GB laptop GPU, Windows, and Cluaiz itself with a cache.
- A model larger than RAM at 8 or 4 GB.
- Repeats: each tuner setting ran once; run 13a's numbers use 128 tokens.
