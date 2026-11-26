# SSD expert prefetch A/B, laptop (run 20 repeated)

Model google_gemma-4-26B-A4B-it-IQ4_XS.gguf, 14205004928 bytes; 3 rounds, rotated order; pp512 tg128, -r 3, 8 threads, n_cpu_moe 30 for L and C.
Speeds are each tool's own table (mean and sd over the repetitions). Disk and RAM come from typeperf, one sample per second over the whole process (load included).

| Run | Config | exit | pp t/s | tg t/s | min free RAM MiB | disk read MB/s (mean) | disk read GB | max read queue | peak GPU dedicated MiB | placement (Cluaiz -vv) | MoE cache MiB | prefetch |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| warmup | warm-up | 0 |  | 7.33 +- 0.00 | 3858 | 482.6 | 10.62 | 574.12 | 3205 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_L | L | 0 | 237.26 +- 35.71 | 12.89 +- 7.33 | 1884 | 44.3 | 3.23 | 25.33 | 3110 |  |  |  |
| r1_C | C | 0 | 183.71 +- 64.82 | 14.78 +- 1.64 | 715 | 205.8 | 17.29 | 464.6 | 3463 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-plan | C-plan | 0 | 174.55 +- 86.70 | 17.39 +- 0.50 | 1400 | 147.2 | 13.54 | 231.23 | 3559 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-cache | C-cache | 0 | 185.75 +- 81.15 | 20.09 +- 0.93 | 1419 | 108.5 | 7.16 | 187.98 | 3647 | ngl -1 ncmoe 30 ctx 1024 Kv16 | 402.47 |  |
| r1_C-pf | C-pf | 0 | 297.59 +- 36.60 | 10.23 +- 0.30 | 1001 | 12.3 | 0.74 | 7.27 | 3820 | ngl -1 ncmoe 30 ctx 1024 Kv16 | 451.62 | 11548 calls, 173143 MiB |
| r2_C | C | 0 | 196.87 +- 38.10 | 16.79 +- 0.95 | 744 | 77.9 | 4.36 | 147.79 | 3450 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-plan | C-plan | 0 | 323.07 +- 54.36 | 18.39 +- 0.35 | 765 | 95 | 5.41 | 4226.06 | 3787 | ngl -1 ncmoe 29 ctx 1024 Kv16 |  |  |
| r2_C-cache | C-cache | 0 | 172.20 +- 71.51 | 17.48 +- 0.49 | 890 | 120 | 9.72 | 162.46 | 3500 | ngl -1 ncmoe 30 ctx 1024 Kv16 | 273.43 |  |
| r2_C-pf | C-pf | 0 | 177.23 +- 76.71 | 8.68 +- 0.05 | 669 | 89.5 | 8.86 | 151.32 | 3588 | ngl -1 ncmoe 30 ctx 1024 Kv16 | 258.07 | 11554 calls, 215027 MiB |
| r2_L | L | 0 | 140.97 +- 34.17 | 15.73 +- 0.69 | 390 | 136.1 | 13.88 | 83.2 | 3329 |  |  |  |
| r3_C-plan | C-plan | 0 | 168.95 +- 65.93 | 15.92 +- 0.64 | 433 | 125.2 | 11.77 | 40.93 | 3354 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C-cache | C-cache | 0 | 167.87 +- 75.52 | 15.72 +- 0.20 | 470 | 124.6 | 12.21 | 123.71 | 3472 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C-pf | C-pf | 0 | 159.42 +- 68.19 | 14.98 +- 0.86 | 394 | 128.2 | 12.31 | 107.66 | 3509 | ngl 30 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_L | L | 0 | 138.34 +- 38.53 | 17.16 +- 0.58 | 63 | 168.6 | 18.04 | 68.34 | 3596 |  |  |  |
| r3_C | C | 0 | 188.85 +- 81.33 | 15.97 +- 0.75 | 983 | 105.7 | 7.61 | 123.41 | 3747 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |

Median of the rounds (range in brackets):

| Config | pp t/s | tg t/s | run 20 tg t/s (rounds 2-3, release build of 16cd9a5a) |
|---|---|---|---|
| L | 140.97 (138.34-237.26) | 15.73 (12.89-17.16) | 15.9 (llama-bench b12428) |
| C | 188.85 (183.71-196.87) | 15.97 (14.78-16.79) | 14.9 (cluaiz bench, same flags) |
| C-plan | 174.55 (168.95-323.07) | 17.39 (15.92-18.39) |  |
| C-cache | 172.20 (167.87-185.75) | 17.48 (15.72-20.09) |  |
| C-pf | 177.23 (159.42-297.59) | 10.23 (8.68-14.98) |  |

WARNING: 2 C-cache/C-pf runs show no 'MoE cache size' line: the planner gave no MoE cache, so those rows measure no cache and no prefetch.
WARNING: 1 C-pf runs show no prefetch line: the prefetch did not run there.
Run 20 reference: test/cloud/runs/2026-11-16-run20-laptop-moe-disk/SUMMARY.md (round 1 there: 13.66 and 14.35 t/s).
