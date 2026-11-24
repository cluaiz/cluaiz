# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-1.7b | W1 | soak | 32 | 555/555 | 73.376 | 74.575 | 72.479 | 2.7 | 6.75 | 0 | BREAK: p95=74.575s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 0.414 / 0.6 | 1131.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 75.5 | 0/14 | 6.64 / 13.442 / 20.69 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 2.8 / 2.8 | 1133.6 | 0.0 | 0.0 | 0.0 | 1.0 / 6.5 | 75.5 | 0/1 | 22.0 / 22.0 / 22.0 | 0 |
| native_cluaiz_W1_qwen3-1.7b_soak_c32 | 1255 | 12.846 / 26.4 | 2425.6 | 0.0 | 0.0 | 97.849 | 1173.1 / 1085.1 | 92.2 | 0/1255 | 15.05 / 19.153 / 318.62 | 0 |
| cooldown | 6 | 0.3 / 0.6 | 1126.0 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 76.1 | 0/6 | 6.54 / 6.715 / 6.77 | 0 |
