# SSD expert prefetch A/B, laptop (run 20 repeated)

Model google_gemma-4-26B-A4B-it-IQ4_XS.gguf, 14205004928 bytes; 6 rounds, rotated order; pp512 tg128, -r 1, 8 threads, n_cpu_moe 30 for L and C.
Speeds are each tool's own table (mean and sd over the repetitions). Disk and RAM come from typeperf, one sample per second over the whole process (load included).

| Run | Config | exit | pp t/s | tg t/s | min free RAM MiB | disk read MB/s (mean) | disk read GB | max read queue | peak GPU dedicated MiB | placement (Cluaiz -vv) | MoE cache MiB | prefetch |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| warmup | warm-up | 0 |  | 0.49 +- 0.00 | 2715 | 214.2 | 6.85 | 5140.93 | 3429 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C | C | 0 | 52.90 +- 0.00 | 3.40 +- 0.00 | 135 | 127.8 | 13.42 | 1548.46 | 3642 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r1_C-lazy | C-lazy | 0 | 4.53 +- 0.00 | 9.93 +- 0.00 | 0 | 193.3 | 36.74 | 632.91 | 3790 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C-lazy | C-lazy | 0 | 7.63 +- 0.00 | 14.06 +- 0.00 | 0 | 128.9 | 25.65 | 857.99 | 3813 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r2_C | C | 0 | 57.18 +- 0.00 | 15.77 +- 0.00 | 1018 | 168.4 | 12.3 | 894.76 | 3544 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C | C | 0 | 75.42 +- 0.00 | 15.17 +- 0.00 | 568 | 178.2 | 14.08 | 387.67 | 3715 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r3_C-lazy | C-lazy | 0 | 271.11 +- 0.00 | 19.92 +- 0.00 | 552 | 10.1 | 0.23 | 0.76 | 3543 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r4_C-lazy | C-lazy | 0 | 262.57 +- 0.00 | 18.06 +- 0.00 | 620 | 6.1 | 0.14 | 0.83 | 3655 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r4_C | C | 0 | 91.69 +- 0.00 | 17.14 +- 0.00 | 659 | 174.9 | 12.07 | 566.09 | 3662 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r5_C | C | 0 | 81.31 +- 0.00 | 14.18 +- 0.00 | 1680 | 163.9 | 12.62 | 286.89 | 3639 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r5_C-lazy | C-lazy | 0 | 75.21 +- 0.00 | 15.03 +- 0.00 | 1293 | 162.5 | 12.19 | 344.16 | 3628 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r6_C-lazy | C-lazy | 0 | 259.14 +- 0.00 | 18.40 +- 0.00 | 1272 | 2.8 | 0.07 | 0.64 | 3568 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |
| r6_C | C | 0 | 94.90 +- 0.00 | 17.89 +- 0.00 | 1167 | 158.7 | 11.75 | 262.97 | 3560 | ngl -1 ncmoe 30 ctx 1024 Kv16 |  |  |

Median of the rounds (range in brackets):

| Config | pp t/s | tg t/s | run 20 tg t/s (rounds 2-3, release build of 16cd9a5a) |
|---|---|---|---|
| C | 78.37 (52.90-94.90) | 15.47 (3.40-17.89) | 14.9 (cluaiz bench, same flags) |
| C-lazy | 167.18 (4.53-271.11) | 16.55 (9.93-19.92) |  |

WARNING: 2 C-cache/C-pf runs show no 'MoE cache size' line: the planner gave no MoE cache, so those rows measure no cache and no prefetch.
WARNING: 1 C-pf runs show no prefetch line: the prefetch did not run there.
Run 20 reference: test/cloud/runs/2026-11-16-run20-laptop-moe-disk/SUMMARY.md (round 1 there: 13.66 and 14.35 t/s).
