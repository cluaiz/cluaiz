# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-1.7b | W1 | soak | 32 | 556/556 | 73.299 | 74.588 | 72.388 | 2.71 | 6.75 | 0 | BREAK: p95=74.588s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 0.35 / 0.6 | 1131.6 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 74.0 | 0/14 | 6.81 / 12.186 / 21.02 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 2.7 / 2.7 | 1132.4 | 0.0 | 0.0 | 0.0 | 1.0 / 6.5 | 74.0 | 0/1 | 18.96 / 18.96 / 18.96 | 0 |
| native_cluaiz_W1_qwen3-1.7b_soak_c32 | 1266 | 12.867 / 29.1 | 2318.8 | 0.0 | 0.0 | 97.909 | 1057.0 / 1085.5 | 90.6 | 0/1266 | 9.03 / 10.723 / 16.39 | 0 |
| cooldown | 6 | 0.4 / 0.7 | 1135.5 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 74.8 | 0/6 | 6.685 / 6.748 / 6.75 | 0 |
