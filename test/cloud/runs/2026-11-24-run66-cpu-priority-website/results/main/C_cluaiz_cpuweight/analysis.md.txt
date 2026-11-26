# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-1.7b | W1 | soak | 32 | 556/556 | 73.273 | 74.643 | 72.348 | 2.71 | 6.75 | 0 | BREAK: p95=74.643s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 0.429 / 0.9 | 1132.0 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 74.8 | 0/14 | 6.495 / 11.989 / 21.05 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 2.8 / 2.8 | 1132.5 | 0.0 | 0.0 | 0.0 | 1.0 / 6.4 | 74.8 | 0/1 | 22.24 / 22.24 / 22.24 | 0 |
| native_cluaiz_W1_qwen3-1.7b_soak_c32 | 1264 | 12.835 / 31.0 | 2359.8 | 0.0 | 0.0 | 97.817 | 1082.6 / 1085.1 | 91.4 | 0/1264 | 6.73 / 10.097 / 20.14 | 0 |
| cooldown | 6 | 0.317 / 0.6 | 1132.1 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 75.5 | 0/6 | 6.4 / 6.54 / 6.54 | 0 |
