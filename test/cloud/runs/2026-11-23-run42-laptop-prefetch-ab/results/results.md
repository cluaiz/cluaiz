# SSD expert prefetch A/B, laptop (run 20 repeated)

Model google_gemma-4-26B-A4B-it-IQ4_XS.gguf, 14205004928 bytes; 3 rounds, rotated order; pp512 tg128, -r 3, 8 threads, n_cpu_moe 30 for L and C.
Speeds are each tool's own table (mean and sd over the repetitions). Disk and RAM come from typeperf, one sample per second over the whole process (load included).

| Run | Config | exit | pp t/s | tg t/s | min free RAM MiB | disk read MB/s (mean) | disk read GB | max read queue | peak GPU dedicated MiB | placement (Cluaiz -vv) | MoE cache MiB | prefetch |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| warmup | warm-up | 0 |  | 2.05 +- 0.00 | 1761 | 244.9 | 6.37 | 4427.41 | 3762 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_L | L | 0 | 126.55 +- 27.03 | 15.94 +- 0.22 | 4 | 115.6 | 15.95 | 4962.64 | 3745 |  |  |  |
| r1_C | C | 0 | 52.43 +- 32.68 | 14.82 +- 1.52 | 836 | 94.9 | 8.92 | 6971.73 | 3761 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-plan | C-plan | 0 | 176.92 +- 76.49 | 16.62 +- 0.58 | 941 | 147.1 | 13.38 | 265.41 | 3614 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-cache | C-cache | 0 | 174.20 +- 77.68 | 15.27 +- 0.39 | 846 | 109.1 | 8.62 | 84.74 | 3524 | ngl 27 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-pf | C-pf | 0 | 180.30 +- 80.61 | 16.66 +- 0.71 | 1332 | 110.2 | 8.04 | 144.06 | 3508 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C | C | 0 | 321.71 +- 55.57 | 17.45 +- 0.06 | 1455 | 20.3 | 0.87 | 7.65 | 3508 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-plan | C-plan | 0 | 325.11 +- 45.38 | 17.16 +- 0.11 | 1494 | 15.8 | 0.69 | 50.78 | 3508 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-cache | C-cache | 0 | 284.45 +- 66.87 | 17.68 +- 0.58 | 1399 | 27 | 1.19 | 6.79 | 3624 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-pf | C-pf | 0 | 333.74 +- 49.34 | 17.35 +- 0.76 | 1457 | 15.6 | 0.66 | 32.33 | 3563 | ngl 30 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_L | L | 0 | 302.17 +- 38.37 | 19.48 +- 0.55 | 1293 | 19.9 | 0.8 | 33.45 | 3492 |  |  |  |
| r3_C-plan | C-plan | 0 | 299.28 +- 52.43 | 16.76 +- 0.41 | 1399 | 22.3 | 0.98 | 3.75 | 3562 | ngl 30 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C-cache | C-cache | 0 | 344.78 +- 43.54 | 17.17 +- 0.63 | 2137 | 8.2 | 0.34 | 2.69 | 3556 | ngl 30 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C-pf | C-pf | 0 | 335.46 +- 53.75 | 17.34 +- 0.50 | 2073 | 8.9 | 0.37 | 0.4 | 3553 | ngl 30 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_L | L | 0 | 320.38 +- 34.45 | 19.68 +- 0.08 | 1936 | 8.9 | 0.34 | 2.96 | 3475 |  |  |  |
| r3_C | C | 0 | 344.03 +- 42.46 | 17.84 +- 0.45 | 2158 | 1.7 | 0.07 | 0.14 | 3590 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |

Median of the rounds (range in brackets):

| Config | pp t/s | tg t/s | run 20 tg t/s (rounds 2-3, release build of 16cd9a5a) |
|---|---|---|---|
| L | 302.17 (126.55-320.38) | 19.48 (15.94-19.68) | 15.9 (llama-bench b12428) |
| C | 321.71 (52.43-344.03) | 17.45 (14.82-17.84) | 14.9 (cluaiz bench, same flags) |
| C-plan | 299.28 (176.92-325.11) | 16.76 (16.62-17.16) |  |
| C-cache | 284.45 (174.20-344.78) | 17.17 (15.27-17.68) |  |
| C-pf | 333.74 (180.30-335.46) | 17.34 (16.66-17.35) |  |

WARNING: 6 C-cache/C-pf runs show no 'MoE cache size' line: the planner gave no MoE cache, so those rows measure no cache and no prefetch.
WARNING: 3 C-pf runs show no prefetch line: the prefetch did not run there.
Run 20 reference: test/cloud/runs/2026-11-16-run20-laptop-moe-disk/SUMMARY.md (round 1 there: 13.66 and 14.35 t/s).
