# vps26 Analysis

## Docker ON vs OFF Baseline
- Docker ON machine used avg: 478.21 MB (Docker daemon RSS avg: 102.08 MB)
- Docker OFF machine used avg: 422.98 MB
- Delta machine used memory: 55.23 MB

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | llama | qwen3-1.7b | W1 | soak | 16 | 1073/1073 | 53.528 | 63.791 | 51.875 | 3.77 | 7.48 | 0 | BREAK: p95=63.791s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 4.621 / 18.9 | 479.7 | 0.214 | 55.8 | 0.0 | 0.0 / 0.0 | 94.0 | 0/14 | 9.8 / 41.092 / 97.2 | 0 |
| native_llama_W1_qwen3-1.7b_start | 8 | 87.475 / 100.0 | 1424.5 | 0.0 | 55.8 | 57.462 | 938.7 / 1072.1 | 75.9 | 0/8 | 16.07 / 140.158 / 203.61 | 0 |
| native_llama_W1_qwen3-1.7b_soak_c16 | 3481 | 99.934 / 100.0 | 1811.2 | 0.103 | 55.8 | 94.268 | 1147.3 / 1062.9 | 100.7 | 0/3481 | 16.47 / 174.11 / 604.0 | 0 |
| cooldown | 6 | 2.5 / 3.0 | 540.9 | 0.167 | 50.1 | 0.0 | 0.0 / 0.0 | 76.1 | 0/6 | 9.205 / 10.285 / 10.32 | 0 |
