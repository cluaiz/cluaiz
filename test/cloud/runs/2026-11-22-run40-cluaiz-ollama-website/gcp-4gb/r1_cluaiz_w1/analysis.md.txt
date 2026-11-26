# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-0.6b | W1 | ramp | 1 | 55/55 | 1.582 | 1.855 | 0.781 | 6.06 | 12.61 | 0 | PASS json=1.0 acc=0.3091 |
| native | cluaiz | qwen3-0.6b | W1 | ramp | 4 | 60/60 | 6.244 | 7.045 | 5.471 | 6.27 | 12.66 | 0 | PASS json=1.0 acc=0.3 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 1 | 26/26 | 3.278 | 4.045 | 2.11 | 1.76 | 5.22 | 0 | PASS json=1.0 acc=0.8462 |
| native | cluaiz | qwen3-1.7b | W1 | ramp | 4 | 31/31 | 13.249 | 14.231 | 11.959 | 1.89 | 5.31 | 0 | PASS json=1.0 acc=0.8387 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 19.45 / 27.2 | 680.7 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 155.8 | 0/14 | 5.04 / 17.493 / 33.34 | 0 |
| native_cluaiz_W1_qwen3-0.6b_start | 2 | 39.65 / 40.1 | 681.2 | 0.0 | 0.0 | 21.55 | 7.7 / 32.7 | 155.8 | 0/2 | 9.155 / 13.03 / 13.46 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c1 | 88 | 96.008 / 98.6 | 1195.7 | 0.0 | 0.0 | 151.741 | 533.3 / 414.0 | 156.4 | 0/88 | 9.16 / 20.652 / 29.34 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c4 | 96 | 94.781 / 98.5 | 1238.9 | 0.0 | 0.0 | 149.67 | 559.6 / 410.5 | 156.3 | 0/96 | 8.96 / 17.543 / 24.79 | 0 |
| native_cluaiz_W1_qwen3-1.7b_start | 1 | 31.5 / 31.5 | 697.2 | 0.0 | 0.0 | 0.0 | 0.3 / 3.0 | 134.6 | 0/1 | 4.65 / 4.65 / 4.65 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c1 | 91 | 96.721 / 99.5 | 1633.1 | 0.0 | 0.0 | 155.62 | 977.4 / 1092.7 | 158.8 | 0/91 | 9.9 / 21.245 / 30.57 | 0 |
| native_cluaiz_W1_qwen3-1.7b_ramp_c4 | 103 | 96.453 / 99.5 | 1667.4 | 0.0 | 0.0 | 155.634 | 1003.5 / 1085.3 | 180.8 | 0/103 | 9.42 / 20.046 / 22.05 | 0 |
| cooldown | 6 | 16.267 / 19.9 | 685.7 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 158.0 | 0/6 | 5.025 / 16.37 / 19.25 | 0 |
