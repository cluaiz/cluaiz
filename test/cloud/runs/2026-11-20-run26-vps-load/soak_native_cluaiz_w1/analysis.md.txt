# vps26 Analysis

## Docker ON vs OFF Baseline
- Docker ON machine used avg: 478.21 MB (Docker daemon RSS avg: 102.08 MB)
- Docker OFF machine used avg: 422.98 MB
- Delta machine used memory: 55.23 MB

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-1.7b | W1 | soak | 32 | 1048/1048 | 112.746 | 118.306 | 111.703 | 1.75 | 5.9 | 0 | BREAK: p95=118.306s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 3.179 / 9.9 | 505.7 | 0.143 | 55.4 | 0.0 | 0.0 / 0.0 | 93.5 | 0/14 | 8.24 / 46.341 / 87.59 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 2 | 43.25 / 67.5 | 505.6 | 0.0 | 55.5 | 0.0 | 7.8 / 31.6 | 75.5 | 0/2 | 14.32 / 16.318 / 16.54 | 0 |
| native_cluaiz_W1_qwen3-1.7b_soak_c32 | 3538 | 99.95 / 100.0 | 1759.4 | 0.111 | 55.5 | 95.764 | 1138.6 / 1090.6 | 113.9 | 1/3538 | 15.56 / 172.921 / 5005.5 | 0 |
| cooldown | 6 | 3.117 / 5.8 | 543.3 | 0.167 | 50.0 | 0.0 | 0.0 / 0.0 | 75.8 | 0/6 | 8.43 / 15.92 / 16.72 | 0 |
