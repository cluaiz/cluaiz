# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 16.743 / 20.3 | 687.7 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 158.4 | 0/14 | 4.765 / 6.944 / 9.44 | 0 |
| native_llama_W1_qwen3-0.6b_start | 95 | 18.609 / 51.2 | 718.9 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 159.6 | 0/95 | 4.55 / 8.894 / 12.4 | 0 |
| native_llama_W1_qwen3-1.7b_start | 96 | 18.312 / 50.2 | 703.8 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 159.7 | 0/96 | 4.67 / 8.415 / 11.67 | 0 |
| cooldown | 6 | 15.55 / 23.3 | 695.5 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 137.0 | 0/6 | 4.54 / 5.207 / 5.3 | 0 |
