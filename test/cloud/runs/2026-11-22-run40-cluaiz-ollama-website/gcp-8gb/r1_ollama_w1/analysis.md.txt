# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | ollama | qwen3-0.6b | W1 | ramp | 1 | 70/70 | 1.315 | 1.82 | 0.744 | 10.95 | 23.51 | 0 | PASS json=1.0 acc=0.2857 |
| native | ollama | qwen3-0.6b | W1 | ramp | 4 | 83/83 | 4.477 | 5.471 | 3.814 | 12.73 | 23.53 | 0 | PASS json=1.0 acc=0.2651 |
| native | ollama | qwen3-1.7b | W1 | ramp | 1 | 26/26 | 3.712 | 4.478 | 2.21 | 3.68 | 9.21 | 0 | PASS json=1.0 acc=0.8462 |
| native | ollama | qwen3-1.7b | W1 | ramp | 4 | 40/40 | 9.925 | 12.945 | 8.372 | 5.25 | 9.27 | 0 | PASS json=1.0 acc=0.875 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 12.407 / 21.2 | 759.2 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.5 | 0/14 | 3.485 / 3.826 / 3.93 | 0 |
| native_ollama_W1_qwen3-0.6b_start | 1 | 27.7 / 27.7 | 753.2 | 0.0 | 0.0 | 0.0 | 8.5 / 26.6 | 137.5 | 0/1 | 4.24 / 4.24 / 4.24 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c1 | 88 | 98.766 / 100.0 | 1530.0 | 0.0 | 0.0 | 171.76 | 777.9 / 43.8 | 160.5 | 0/88 | 6.66 / 15.006 / 19.0 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c4 | 95 | 97.649 / 100.0 | 1550.9 | 0.0 | 0.0 | 170.767 | 787.6 / 43.9 | 160.5 | 0/95 | 6.29 / 11.583 / 27.12 | 0 |
| native_ollama_W1_qwen3-1.7b_start | 1 | 38.6 / 38.6 | 768.7 | 0.0 | 0.0 | 0.0 | 10.5 / 27.2 | 136.8 | 0/1 | 3.54 / 3.54 / 3.54 | 0 |
| native_ollama_W1_qwen3-1.7b_ramp_c1 | 89 | 98.237 / 100.0 | 2179.2 | 0.0 | 0.0 | 171.427 | 1433.5 / 45.2 | 181.6 | 0/89 | 6.34 / 14.334 / 21.15 | 0 |
| native_ollama_W1_qwen3-1.7b_ramp_c4 | 99 | 97.91 / 100.0 | 2199.1 | 0.0 | 0.0 | 171.29 | 1448.3 / 45.3 | 160.4 | 0/99 | 6.52 / 12.666 / 45.07 | 0 |
| cooldown | 6 | 12.4 / 16.8 | 757.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 137.5 | 0/6 | 3.555 / 4.018 / 4.16 | 0 |
