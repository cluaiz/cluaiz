# vps26 Analysis

## Docker ON vs OFF Baseline
- Docker ON machine used avg: 478.21 MB (Docker daemon RSS avg: 102.08 MB)
- Docker OFF machine used avg: 422.98 MB
- Delta machine used memory: 55.23 MB

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| docker | cluaiz | qwen3-0.6b | W1 | ramp | 1 | 39/39 | 0.612 | 0.739 | 0.337 | 15.24 | 36.14 | 0 | PASS json=1.0 acc=0.3333 |
| docker | cluaiz | qwen3-0.6b | W1 | ramp | 2 | 41/41 | 1.228 | 1.885 | 0.948 | 15.6 | 35.54 | 0 | PASS json=1.0 acc=0.3659 |
| docker | llama | qwen3-0.6b | W1 | ramp | 1 | 39/39 | 0.702 | 0.817 | 0.39 | 22.78 | 48.36 | 0 | PASS json=1.0 acc=0.3333 |
| docker | llama | qwen3-0.6b | W1 | ramp | 2 | 46/46 | 1.113 | 1.513 | 0.811 | 26.26 | 47.12 | 0 | PASS json=1.0 acc=0.3478 |
| docker | ollama | qwen3-0.6b | W1 | ramp | 1 | 36/36 | 0.694 | 0.813 | 0.387 | 21.37 | 49.06 | 0 | PASS json=1.0 acc=0.3333 |
| docker | ollama | qwen3-0.6b | W1 | ramp | 2 | 46/46 | 1.136 | 1.482 | 0.831 | 26.48 | 47.71 | 0 | PASS json=1.0 acc=0.3478 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 2.207 / 4.0 | 576.3 | 0.071 | 55.0 | 0.0 | 0.0 / 0.0 | 79.4 | 0/14 | 8.305 / 17.299 / 31.95 | 0 |
| docker_cluaiz_W1_qwen3-0.6b_start | 2 | 48.6 / 81.2 | 618.3 | 4.95 | 56.9 | 0.0 | 11.4 / 22.4 | 79.4 | 0/2 | 13.32 / 14.904 / 15.08 | 0 |
| docker_cluaiz_W1_qwen3-0.6b_ramp_c1 | 25 | 98.732 / 100.0 | 1160.8 | 0.24 | 57.2 | 94.88 | 565.1 / 412.3 | 79.4 | 0/25 | 14.16 / 16.714 / 18.11 | 0 |
| docker_cluaiz_W1_qwen3-0.6b_ramp_c2 | 29 | 92.138 / 100.0 | 1186.4 | 0.379 | 57.5 | 86.979 | 592.8 / 410.0 | 79.4 | 0/29 | 16.1 / 18.754 / 27.47 | 0 |
| docker_ollama_W1_qwen3-0.6b_start | 2 | 48.7 / 55.0 | 801.9 | 2.5 | 57.6 | 13.35 | 141.5 / 163.1 | 79.4 | 0/2 | 9.99 / 11.718 / 11.91 | 0 |
| docker_ollama_W1_qwen3-0.6b_ramp_c1 | 25 | 98.3 / 100.0 | 1464.9 | 0.44 | 58.7 | 91.816 | 868.0 / 144.3 | 79.4 | 0/25 | 15.83 / 17.184 / 20.49 | 0 |
| docker_ollama_W1_qwen3-0.6b_ramp_c2 | 29 | 91.379 / 100.0 | 1480.7 | 0.517 | 59.6 | 86.569 | 873.2 / 144.3 | 95.7 | 0/29 | 15.05 / 20.674 / 23.95 | 0 |
| docker_llama_W1_qwen3-0.6b_start | 2 | 78.0 / 100.0 | 1091.8 | 3.0 | 59.6 | 47.4 | 497.6 / 388.9 | 79.4 | 0/2 | 12.485 / 15.325 / 15.64 | 0 |
| docker_llama_W1_qwen3-0.6b_ramp_c1 | 25 | 98.496 / 100.0 | 1243.1 | 0.28 | 59.6 | 94.92 | 589.1 / 389.2 | 96.9 | 0/25 | 14.33 / 16.234 / 17.03 | 0 |
| docker_llama_W1_qwen3-0.6b_ramp_c2 | 29 | 92.217 / 100.0 | 1243.1 | 0.31 | 59.7 | 86.966 | 594.1 / 389.2 | 79.4 | 0/29 | 15.27 / 19.256 / 28.24 | 0 |
| cooldown | 6 | 1.833 / 2.0 | 617.9 | 0.0 | 59.7 | 0.0 | 0.0 / 0.0 | 79.4 | 0/6 | 8.605 / 8.905 / 9.0 | 0 |
