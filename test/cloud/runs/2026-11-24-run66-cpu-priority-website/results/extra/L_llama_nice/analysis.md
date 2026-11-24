# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | llama | qwen3-1.7b | W1 | soak | 32 | 498/498 | 81.41 | 87.375 | 79.959 | 5.0 | 8.67 | 0 | BREAK: p95=87.375s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 0.45 / 0.9 | 1117.5 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 70.2 | 0/14 | 6.465 / 12.536 / 20.59 | 0 |
| native_llama_W1_qwen3-1.7b_start | 3 | 9.367 / 12.7 | 2083.4 | 0.0 | 0.0 | 63.667 | 966.1 / 1072.6 | 70.2 | 0/3 | 6.64 / 17.323 / 18.51 | 0 |
| native_llama_W1_qwen3-1.7b_soak_c32 | 1272 | 12.776 / 26.8 | 2422.0 | 0.0 | 0.0 | 97.921 | 1142.3 / 1063.3 | 90.3 | 0/1272 | 8.96 / 9.97 / 29.47 | 0 |
| cooldown | 6 | 0.3 / 0.5 | 1148.6 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 74.2 | 0/6 | 6.605 / 6.678 / 6.69 | 0 |
