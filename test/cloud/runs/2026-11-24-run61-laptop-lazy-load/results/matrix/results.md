# SSD expert prefetch A/B, laptop (run 20 repeated)

Model google_gemma-4-26B-A4B-it-IQ4_XS.gguf, 14205004928 bytes; 3 rounds, rotated order; pp512 tg128, -r 1, 8 threads, n_cpu_moe 30 for L and C.
Speeds are each tool's own table (mean and sd over the repetitions). Disk and RAM come from typeperf, one sample per second over the whole process (load included).

| Run | Config | exit | pp t/s | tg t/s | min free RAM MiB | disk read MB/s (mean) | disk read GB | max read queue | peak GPU dedicated MiB | placement (Cluaiz -vv) | MoE cache MiB | prefetch |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| warmup | warm-up | 0 |  | 0.31 +- 0.00 | 2035 | 80.9 | 7.28 | 3273.93 | 3813 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_L | L | 0 | 13.99 +- 0.00 | 1.99 +- 0.00 | 11 | 97.2 | 19.92 | 3369.69 | 3629 |  |  |  |
| r1_C | C | 0 | 52.51 +- 0.00 | 12.98 +- 0.00 | 17 | 166.1 | 9.14 | 5209.28 | 3833 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-lazy | C-lazy | 0 | 53.54 +- 0.00 | 12.47 +- 0.00 | 2 | 175.1 | 12.25 | 264.74 | 3683 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-cache | C-cache | 0 | 76.64 +- 0.00 | 12.53 +- 0.00 | 1291 | 158.4 | 12.99 | 398.14 | 3560 | ngl 28 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-pf | C-pf | 0 | 68.99 +- 0.00 | 14.77 +- 0.00 | 511 | 178.5 | 15.35 | 245.27 | 3430 | ngl 25 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-lazy-pf | C-lazy-pf | 0 | 110.33 +- 0.00 | 16.61 +- 0.00 | 521 | 158.1 | 5.69 | 207.29 | 3447 | ngl 27 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C | C | 0 | 80.71 +- 0.00 | 17.19 +- 0.00 | 475 | 170.6 | 12.46 | 350 | 3573 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-lazy | C-lazy | 0 | 278.32 +- 0.00 | 19.06 +- 0.00 | 499 | 16 | 0.4 | 0.39 | 3471 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-cache | C-cache | 0 | 53.30 +- 0.00 | 13.02 +- 0.00 | 361 | 174.3 | 15.51 | 334.13 | 3535 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-pf | C-pf | 0 | 80.80 +- 0.00 | 7.49 +- 0.00 | 271 | 198.2 | 10.31 | 5279.8 | 3665 | ngl 28 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-lazy-pf | C-lazy-pf | 0 | 24.76 +- 0.00 | 3.90 +- 0.00 | 250 | 101.4 | 14.3 | 240.29 | 3549 | ngl 24 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_L | L | 0 | 35.89 +- 0.00 | 7.50 +- 0.00 | 279 | 95.7 | 10.33 | 6054.06 | 3736 |  |  |  |
| r3_C-lazy | C-lazy | 0 | 104.43 +- 0.00 | 15.54 +- 0.00 | 245 | 161.8 | 11 | 455.71 | 3644 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C-cache | C-cache | 0 | 76.50 +- 0.00 | 17.88 +- 0.00 | 399 | 159 | 6.36 | 6388.71 | 3513 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C-pf | C-pf | 0 | 19.55 +- 0.00 | 6.12 +- 0.00 | 719 | 136 | 13.47 | 341.15 | 3680 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C-lazy-pf | C-lazy-pf | 0 | 243.80 +- 0.00 | 17.42 +- 0.00 | 499 | 12.4 | 0.31 | 0.48 | 3557 | ngl 30 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_L | L | 0 | 124.94 +- 0.00 | 16.72 +- 0.00 | 824 | 170 | 13.09 | 197.31 | 3543 |  |  |  |
| r3_C | C | 0 | 91.87 +- 0.00 | 17.36 +- 0.00 | 679 | 178.9 | 12.7 | 287.54 | 3696 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |

Median of the rounds (range in brackets):

| Config | pp t/s | tg t/s | run 20 tg t/s (rounds 2-3, release build of 16cd9a5a) |
|---|---|---|---|
| L | 35.89 (13.99-124.94) | 7.50 (1.99-16.72) | 15.9 (llama-bench b12428) |
| C | 80.71 (52.51-91.87) | 17.19 (12.98-17.36) | 14.9 (cluaiz bench, same flags) |
| C-lazy | 104.43 (53.54-278.32) | 15.54 (12.47-19.06) |  |
| C-cache | 76.50 (53.30-76.64) | 13.02 (12.53-17.88) |  |
| C-pf | 68.99 (19.55-80.80) | 7.49 (6.12-14.77) |  |
| C-lazy-pf | 110.33 (24.76-243.80) | 16.61 (3.90-17.42) |  |

WARNING: 6 C-cache/C-pf runs show no 'MoE cache size' line: the planner gave no MoE cache, so those rows measure no cache and no prefetch.
WARNING: 3 C-pf runs show no prefetch line: the prefetch did not run there.
Run 20 reference: test/cloud/runs/2026-11-16-run20-laptop-moe-disk/SUMMARY.md (round 1 there: 13.66 and 14.35 t/s).
