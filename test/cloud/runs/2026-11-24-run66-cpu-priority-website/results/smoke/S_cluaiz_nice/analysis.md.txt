# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-1.7b | W1 | soak | 32 | 57/57 | 68.245 | 71.766 | 67.35 | 2.71 | 6.74 | 0 | BREAK: p95=71.766s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 0.393 / 1.4 | 1186.2 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 70.1 | 0/14 | 6.475 / 12.692 / 21.5 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 2.5 / 2.5 | 1162.6 | 0.0 | 0.0 | 0.0 | 1.0 / 6.5 | 70.1 | 0/1 | 11.33 / 11.33 / 11.33 | 0 |
| native_cluaiz_W1_qwen3-1.7b_soak_c32 | 134 | 12.689 / 17.1 | 2220.5 | 0.0 | 0.0 | 95.438 | 1053.9 / 1085.5 | 86.3 | 0/134 | 9.305 / 15.603 / 231.99 | 0 |
| cooldown | 6 | 0.333 / 0.4 | 1131.3 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 70.2 | 0/6 | 6.55 / 6.688 / 6.73 | 0 |
