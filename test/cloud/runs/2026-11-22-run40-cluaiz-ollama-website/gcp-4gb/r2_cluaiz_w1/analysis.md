# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-0.6b | W1 | ramp | 1 | 57/57 | 1.542 | 1.803 | 0.787 | 6.24 | 12.98 | 0 | PASS json=1.0 acc=0.2982 |
| native | cluaiz | qwen3-0.6b | W1 | ramp | 4 | 62/62 | 6.122 | 6.982 | 5.363 | 6.43 | 13.05 | 0 | PASS json=1.0 acc=0.2903 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 1 | 26/26 | 3.218 | 3.933 | 2.101 | 1.8 | 5.43 | 0 | PASS json=1.0 acc=0.8462 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 4 | 31/31 | 13.166 | 14.228 | 11.92 | 1.91 | 5.42 | 0 | PASS json=1.0 acc=0.8387 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 15.979 / 23.4 | 696.9 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 159.7 | 0/14 | 4.4 / 5.54 / 6.32 | 0 |
| native_cluaiz_W1_qwen3-0.6b_start | 1 | 31.2 / 31.2 | 682.0 | 0.0 | 0.0 | 0.0 | 4.2 / 23.9 | 159.9 | 0/1 | 5.0 / 5.0 / 5.0 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c1 | 89 | 95.606 / 98.1 | 1194.9 | 0.0 | 0.0 | 153.975 | 533.2 / 410.2 | 182.4 | 0/89 | 8.06 / 17.052 / 25.51 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c4 | 97 | 94.655 / 98.0 | 1209.3 | 0.0 | 0.0 | 152.426 | 559.5 / 410.2 | 161.0 | 0/97 | 8.03 / 16.282 / 21.8 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 32.8 / 32.8 | 683.6 | 0.0 | 0.0 | 0.0 | 4.2 / 24.4 | 160.5 | 0/1 | 4.62 / 4.62 / 4.62 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c1 | 89 | 96.417 / 99.0 | 1630.6 | 0.0 | 0.0 | 157.239 | 977.4 / 1091.9 | 160.5 | 0/89 | 8.73 / 16.938 / 21.05 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c4 | 103 | 95.578 / 99.1 | 1657.4 | 0.0 | 0.0 | 154.394 | 1003.7 / 1084.1 | 160.3 | 0/103 | 8.23 / 18.283 / 27.02 | 0 |
| cooldown | 6 | 16.5 / 21.2 | 661.6 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.4 | 0/6 | 4.435 / 15.363 / 18.38 | 0 |
