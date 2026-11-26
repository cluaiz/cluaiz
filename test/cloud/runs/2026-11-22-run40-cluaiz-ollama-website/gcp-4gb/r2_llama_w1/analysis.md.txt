# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 16.186 / 22.8 | 669.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 159.4 | 0/14 | 4.815 / 6.332 / 6.41 | 0 |
| native_llama_W1_qwen3-0.6b_start | 96 | 18.336 / 42.3 | 674.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.8 | 0/96 | 4.475 / 8.335 / 17.73 | 0 |
| native_llama_W1_qwen3-1.7b_start | 95 | 17.576 / 39.7 | 684.9 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.7 | 0/95 | 4.43 / 9.493 / 13.21 | 0 |
| cooldown | 6 | 15.567 / 17.7 | 660.1 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 137.3 | 0/6 | 4.595 / 5.027 / 5.15 | 0 |
