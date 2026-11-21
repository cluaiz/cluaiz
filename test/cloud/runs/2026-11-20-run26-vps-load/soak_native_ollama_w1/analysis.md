# vps26 Analysis

## Docker ON vs OFF Baseline
- Docker ON machine used avg: 478.21 MB (Docker daemon RSS avg: 102.08 MB)
- Docker OFF machine used avg: 422.98 MB
- Delta machine used memory: 55.23 MB

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | ollama | qwen3-1.7b | W1 | soak | 16 | 1102/1102 | 52.396 | 60.795 | 50.721 | 3.87 | 7.59 | 0 | BREAK: p95=60.795s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 2.136 / 3.0 | 481.5 | 0.143 | 55.2 | 0.0 | 0.0 / 0.0 | 97.5 | 0/14 | 8.22 / 11.323 / 14.45 | 0 |
| native_ollama_W1_qwen3-1.7b_start | 2 | 62.85 / 100.0 | 490.5 | 0.5 | 54.2 | 0.0 | 10.9 / 27.4 | 75.8 | 0/2 | 17.84 / 19.244 / 19.4 | 0 |
| native_ollama_W1_qwen3-1.7b_soak_c16 | 3469 | 99.891 / 100.0 | 2055.0 | 0.108 | 54.2 | 95.604 | 1553.7 / 44.7 | 99.9 | 0/3469 | 15.98 / 172.222 / 274.44 | 0 |
| cooldown | 5 | 2.2 / 3.0 | 526.9 | 0.0 | 50.1 | 0.0 | 0.0 / 0.0 | 75.9 | 0/5 | 8.46 / 9.836 / 9.89 | 0 |
