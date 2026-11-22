# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | ollama | qwen3-0.6b | W1 | ramp | 1 | 71/71 | 1.314 | 1.658 | 0.769 | 11.12 | 23.65 | 0 | PASS json=1.0 acc=0.2817 |
| native | ollama | qwen3-0.6b | W1 | ramp | 4 | 83/83 | 4.52 | 5.578 | 3.861 | 12.63 | 23.37 | 0 | PASS json=1.0 acc=0.2651 |
| native | ollama | qwen3-1.7b | W1 | ramp | 1 | 26/26 | 3.726 | 4.461 | 2.172 | 3.74 | 9.24 | 0 | PASS json=1.0 acc=0.8462 |
| native | ollama | qwen3-1.7b | W1 | ramp | 4 | 40/40 | 10.152 | 12.958 | 8.57 | 5.15 | 9.21 | 0 | PASS json=1.0 acc=0.875 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 14.336 / 19.4 | 754.8 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.4 | 0/14 | 3.365 / 11.625 / 17.67 | 0 |
| native_ollama_W1_qwen3-0.6b_start | 1 | 26.2 / 26.2 | 759.8 | 0.0 | 0.0 | 0.0 | 10.3 / 26.8 | 160.4 | 0/1 | 6.31 / 6.31 / 6.31 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c1 | 88 | 98.88 / 100.0 | 1531.3 | 0.0 | 0.0 | 172.625 | 777.9 / 43.6 | 182.8 | 0/88 | 6.565 / 10.266 / 24.89 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c4 | 95 | 97.994 / 100.0 | 1544.5 | 0.0 | 0.0 | 170.676 | 787.5 / 43.7 | 161.4 | 0/95 | 6.27 / 9.963 / 15.83 | 0 |
| native_ollama_W1_qwen3-1.7b_start | 1 | 25.9 / 25.9 | 757.2 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.9 | 0/1 | 3.8 / 3.8 / 3.8 | 0 |
| native_ollama_W1_qwen3-1.7b_ramp_c1 | 88 | 98.341 / 100.0 | 2192.6 | 0.0 | 0.0 | 172.143 | 1433.7 / 44.6 | 162.3 | 0/88 | 6.93 / 13.276 / 15.68 | 0 |
| native_ollama_W1_qwen3-1.7b_ramp_c4 | 101 | 97.893 / 100.0 | 2205.3 | 0.0 | 0.0 | 170.486 | 1447.9 / 44.7 | 162.1 | 0/101 | 6.34 / 15.15 / 49.27 | 0 |
| cooldown | 6 | 12.45 / 14.4 | 754.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.2 | 0/6 | 3.415 / 3.912 / 4.05 | 0 |
