# vps26 Analysis

## Docker ON vs OFF Baseline
- Docker ON machine used avg: 478.21 MB (Docker daemon RSS avg: 102.08 MB)
- Docker OFF machine used avg: 422.98 MB
- Delta machine used memory: 55.23 MB

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-0.6b | W1 | ramp | 1 | 186/186 | 0.625 | 0.782 | 0.343 | 15.36 | 34.69 | 0 | PASS json=1.0 acc=0.2849 |
| native | cluaiz | qwen3-0.6b | W1 | ramp | 4 | 190/190 | 2.532 | 2.871 | 2.252 | 15.47 | 34.36 | 0 | PASS json=1.0 acc=0.2789 |
| native | cluaiz | qwen3-0.6b | W1 | soak | 4 | 5564/5564 | 2.56 | 2.962 | 2.273 | 15.32 | 33.93 | 0 | PASS json=1.0 acc=0.2852 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 1 | 81/81 | 1.383 | 1.893 | 0.942 | 4.18 | 14.36 | 0 | PASS json=1.0 acc=0.8519 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 4 | 93/93 | 5.341 | 6.082 | 4.88 | 4.62 | 14.93 | 0 | PASS json=1.0 acc=0.828 |
| native | cluaiz | qwen3-1.7b | W1 | soak | 4 | 2726/2726 | 5.248 | 6.053 | 4.842 | 4.69 | 15.13 | 0 | PASS json=1.0 acc=0.8158 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 2.271 / 4.0 | 519.2 | 0.143 | 47.6 | 0.0 | 0.0 / 0.0 | 74.2 | 0/14 | 8.925 / 18.428 / 33.3 | 0 |
| native_cluaiz_W1_qwen3-0.6b_start | 1 | 17.6 / 17.6 | 526.7 | 0.0 | 47.1 | 0.0 | 0.0 / 0.0 | 57.4 | 0/1 | 16.15 / 16.15 / 16.15 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c1 | 119 | 99.619 / 100.0 | 1081.1 | 0.042 | 47.1 | 96.052 | 532.2 / 407.0 | 72.5 | 0/119 | 15.91 / 22.054 / 42.04 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c4 | 120 | 100.0 / 100.0 | 1080.2 | 0.058 | 44.4 | 97.484 | 559.0 / 407.0 | 72.5 | 0/120 | 15.745 / 19.405 / 24.13 | 0 |
| native_cluaiz_W1_qwen3-0.6b_soak_c4 | 3556 | 99.934 / 100.0 | 1156.6 | 0.051 | 43.5 | 97.299 | 559.0 / 407.0 | 78.5 | 0/3556 | 15.72 / 20.19 / 114.79 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 26.0 / 26.0 | 567.8 | 0.0 | 41.4 | 0.0 | 7.6 / 28.9 | 56.6 | 0/1 | 8.92 / 8.92 / 8.92 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c1 | 119 | 99.464 / 100.0 | 1567.0 | 0.059 | 41.4 | 95.808 | 975.3 / 1088.6 | 69.8 | 0/119 | 16.24 / 24.416 / 64.67 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c4 | 124 | 100.0 / 100.0 | 1531.9 | 0.04 | 37.9 | 97.69 | 1001.5 / 1069.4 | 71.8 | 0/124 | 15.66 / 18.684 / 46.65 | 0 |
| native_cluaiz_W1_qwen3-1.7b_soak_c4 | 3559 | 99.946 / 100.0 | 1593.4 | 0.05 | 37.9 | 97.572 | 1001.8 / 1069.4 | 77.1 | 0/3559 | 15.49 / 19.408 / 126.93 | 0 |
| cooldown | 6 | 1.817 / 2.9 | 546.3 | 0.0 | 37.8 | 0.0 | 0.0 / 0.0 | 53.5 | 0/6 | 9.41 / 9.92 / 9.92 | 0 |
