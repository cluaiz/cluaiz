# Run 42: run 20 repeated on the same laptop, with the MoE cache and the expert prefetch

Date: 2026-11-23. Script: `test/cloud/remote/ssd_prefetch/run_prefetch_laptop.ps1` (commit `e2da32d2`).
Raw output of every run: `results/` (stdout tables, stderr, the Engine log part, typeperf samples).

## Setup

| | |
|---|---|
| Machine | run 20's laptop: Windows 11, Ryzen 7 7435HS, 23.7 GB RAM, RTX 3050 Laptop 4 GB (4,093 MiB) |
| Model | `bartowski/google_gemma-4-26B-A4B-it-GGUF:IQ4_XS`, 14,205,004,928 bytes, sha256 `86f0f6ca...e8b9` (matches Hugging Face), experts 11.52 GB |
| llama.cpp | official `llama-bench` b12428 (run 20's build) |
| Cluaiz | debug build (`cargo build -p cmd`) of the working tree at `da22cab3`; llama.cpp inside it is built with CMake Release flags |
| Home | isolated `CLUAIZ_HOME`; no server, no port |
| Flags | pp512, tg128, 3 repetitions, 8 threads; L and C: `-ngl 99 -ncmoe 30`, flash attention on, KV f16 |
| Order | warm-up load, then 3 rounds with the order rotated (Windows cannot drop its file cache without an extra tool) |

Configs: L = official llama-bench; C = `cluaiz bench` with run 20's placement; C-plan = the planner's own
placement; C-cache = C-plan + `CLUAIZ_MOE_CACHE=on`; C-pf = C-cache + `CLUAIZ_EXPERT_PREFETCH=on`.

## Results (tg128 t/s, median of 3 rounds, range in brackets)

| Config | tg t/s | pp t/s | run 20 tg t/s |
|---|---|---|---|
| L | 19.48 (15.94-19.68) | 302.17 | 15.9 |
| C | 17.45 (14.82-17.84) | 321.71 | 14.9 |
| C-plan | 16.76 (16.62-17.16) | 299.28 | |
| C-cache | 17.17 (15.27-17.68) | 284.45 | |
| C-pf | 17.34 (16.66-17.35) | 333.74 | |

## What this shows

1. **Round 1 repeats run 20.** L 15.94 and C 14.82 t/s, against run 20's 15.9 and 14.9.
2. **After round 1 the disk is out of the picture.** The model (14.2 GB) fits in this laptop's 23.7 GB RAM,
   so Windows keeps it in its file cache: rounds 2 and 3 read 0.07-1.19 GB from disk per run, round 1 read
   8-16 GB. This laptop and model are not an SSD-bound case once warm.
3. **Generation: Cluaiz is behind llama.cpp when warm.** Rounds 2-3: L 19.48 and 19.68 t/s, C 17.45 and
   17.84 t/s (about 9-10% lower). Run 20 (release build) showed about 6%. The cause is not known (?); the
   debug build of the Rust side is one candidate and has not been tested. Prompt speed (pp) is equal or higher.
4. **The MoE cache and the prefetch never ran.** No C-cache or C-pf run logged a MoE cache size or a
   prefetch line. The plan (`results/r3_C-pf.engine.log`) shows why: 2.85 GB of the 4 GB GPU is free, and
   attention/shared weights + KV + buffers take 2.37-2.41 GB of it, so no VRAM is left for a cache of
   experts. The prefetch runs only inside the cache upload path, so on a 4 GB GPU it cannot run. The C-cache
   and C-pf rows therefore measure the same thing as C-plan; their differences are noise.

## Caveats

- Round 1 may be affected: a separate test server (Qwen3.5-0.8B, port 8013, another task) may still have
  been running during the warm-up and the first L run.
- The owner's own Engine server (port 8000, no model loaded, about 175 MB) ran during the whole test.
- No release build, no cold-cache runs (RAMMap not used).

## Pending

| Item | Why |
|---|---|
| Prefetch speed proof | needs a machine where a MoE cache fits in VRAM and the model does not fit in RAM (Phase 0/2 plan, L4) |
| Prefetch on small GPUs | today it is tied to the GPU cache; a 4 GB GPU never gets it. A design question for the owner |
| Warm tg gap against llama.cpp (9-10%) | repeat C with a release build to see whether the debug build explains it |
