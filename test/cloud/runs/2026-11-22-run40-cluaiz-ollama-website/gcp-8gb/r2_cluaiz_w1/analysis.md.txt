# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-0.6b | W1 | ramp | 1 | 67/67 | 1.316 | 1.585 | 0.711 | 7.35 | 16.45 | 0 | PASS json=1.0 acc=0.2836 |
| native | cluaiz | qwen3-0.6b | W1 | ramp | 4 | 72/72 | 5.273 | 5.745 | 4.676 | 7.51 | 16.67 | 0 | PASS json=1.0 acc=0.2778 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 1 | 31/31 | 2.772 | 3.476 | 1.922 | 2.1 | 7.06 | 0 | PASS json=1.0 acc=0.8387 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 4 | 35/35 | 11.279 | 12.274 | 10.405 | 2.22 | 7.0 | 0 | PASS json=1.0 acc=0.8571 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 12.879 / 18.4 | 745.8 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 137.7 | 0/14 | 3.51 / 4.029 / 4.25 | 0 |
| native_cluaiz_W1_qwen3-0.6b_start | 1 | 26.7 / 26.7 | 743.8 | 0.0 | 0.0 | 0.0 | 4.2 / 23.2 | 137.7 | 0/1 | 6.7 / 6.7 / 6.7 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c1 | 89 | 95.579 / 98.5 | 1285.4 | 0.0 | 0.0 | 161.322 | 533.5 / 407.7 | 160.4 | 0/89 | 5.8 / 10.582 / 15.5 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c4 | 97 | 94.668 / 98.0 | 1316.7 | 0.0 | 0.0 | 158.688 | 559.9 / 407.7 | 161.0 | 0/97 | 5.86 / 10.458 / 19.33 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 16.7 / 16.7 | 758.3 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 138.4 | 0/1 | 3.4 / 3.4 / 3.4 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c1 | 91 | 96.693 / 99.0 | 1740.9 | 0.0 | 0.0 | 164.311 | 977.5 / 1088.9 | 161.5 | 0/91 | 5.98 / 10.59 / 15.92 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c4 | 100 | 95.933 / 99.0 | 1766.4 | 0.0 | 0.0 | 162.391 | 1003.8 / 1081.6 | 182.9 | 0/100 | 6.8 / 13.264 / 18.82 | 0 |
| cooldown | 6 | 13.133 / 14.9 | 760.8 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.2 | 0/6 | 3.41 / 3.56 / 3.58 | 0 |
