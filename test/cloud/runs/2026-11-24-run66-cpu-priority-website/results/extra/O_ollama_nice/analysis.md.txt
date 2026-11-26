# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | ollama | qwen3-1.7b | W1 | soak | 32 | 501/501 | 80.472 | 87.033 | 79.024 | 5.03 | 8.79 | 0 | BREAK: p95=87.033s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 0.407 / 0.9 | 1140.9 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 74.2 | 0/14 | 6.68 / 13.209 / 20.71 | 0 |
| native_ollama_W1_qwen3-1.7b_start | 2 | 8.35 / 14.3 | 1187.2 | 0.0 | 0.0 | 2.95 | 29.5 / 55.7 | 74.2 | 0/2 | 14.9 / 20.192 / 20.78 | 0 |
| native_ollama_W1_qwen3-1.7b_soak_c32 | 1271 | 12.825 / 33.5 | 2816.4 | 0.0 | 0.0 | 97.891 | 1565.0 / 45.3 | 91.1 | 0/1271 | 9.13 / 10.58 / 305.52 | 0 |
| cooldown | 6 | 0.283 / 0.6 | 1130.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 75.0 | 0/6 | 6.615 / 6.75 / 6.78 | 0 |
