# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-1.7b | W1 | soak | 32 | 555/555 | 73.45 | 74.674 | 72.535 | 2.7 | 6.75 | 0 | BREAK: p95=74.674s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 0.314 / 0.5 | 1135.8 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 69.9 | 0/14 | 6.615 / 11.872 / 21.07 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 2.7 / 2.7 | 1136.7 | 0.0 | 0.0 | 0.0 | 1.0 / 6.5 | 69.9 | 0/1 | 25.43 / 25.43 / 25.43 | 0 |
| native_cluaiz_W1_qwen3-1.7b_soak_c32 | 1256 | 12.825 / 27.3 | 2421.5 | 0.0 | 0.0 | 97.857 | 1137.5 / 1085.7 | 90.1 | 0/1256 | 14.965 / 19.32 / 110.24 | 0 |
| cooldown | 6 | 0.3 / 0.5 | 1138.6 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 74.0 | 0/6 | 6.41 / 6.58 / 6.62 | 0 |
