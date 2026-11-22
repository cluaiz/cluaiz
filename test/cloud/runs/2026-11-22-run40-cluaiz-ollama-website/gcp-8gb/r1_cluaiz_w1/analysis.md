# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-0.6b | W1 | ramp | 1 | 64/64 | 1.349 | 1.61 | 0.726 | 7.07 | 16.15 | 0 | PASS json=1.0 acc=0.2812 |
| native | cluaiz | qwen3-0.6b | W1 | ramp | 4 | 71/71 | 5.385 | 5.795 | 4.762 | 7.4 | 16.08 | 0 | PASS json=1.0 acc=0.2817 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 1 | 30/30 | 2.806 | 3.483 | 1.945 | 2.08 | 6.96 | 0 | PASS json=1.0 acc=0.8333 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 4 | 35/35 | 11.394 | 12.451 | 10.52 | 2.21 | 6.89 | 0 | PASS json=1.0 acc=0.8571 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 16.779 / 21.7 | 729.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 133.5 | 0/14 | 4.085 / 5.604 / 5.65 | 0 |
| native_cluaiz_W1_qwen3-0.6b_start | 1 | 30.8 / 30.8 | 729.9 | 0.0 | 0.0 | 0.0 | 1.0 / 4.0 | 133.5 | 0/1 | 4.73 / 4.73 / 4.73 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c1 | 89 | 95.237 / 99.0 | 1266.3 | 0.0 | 0.0 | 158.37 | 533.3 / 411.2 | 177.2 | 0/89 | 6.29 / 11.43 / 25.2 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c4 | 97 | 94.09 / 98.6 | 1277.6 | 0.0 | 0.0 | 154.248 | 533.6 / 407.3 | 156.6 | 0/97 | 6.15 / 15.694 / 27.69 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c1 | 89 | 96.374 / 99.5 | 1712.8 | 0.0 | 0.0 | 162.313 | 977.5 / 1088.4 | 158.6 | 0/89 | 7.03 / 11.418 / 21.66 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c4 | 101 | 95.718 / 99.0 | 1746.8 | 0.0 | 0.0 | 161.756 | 1004.1 / 1080.5 | 180.2 | 0/101 | 6.89 / 14.86 / 24.31 | 0 |
| cooldown | 6 | 14.05 / 16.2 | 752.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 158.7 | 0/6 | 3.73 / 4.555 / 4.82 | 0 |
