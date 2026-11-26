# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 12.586 / 15.7 | 755.2 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 138.7 | 0/14 | 3.505 / 4.867 / 5.25 | 0 |
| native_llama_W1_qwen3-0.6b_start | 95 | 14.641 / 26.2 | 758.6 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.9 | 0/95 | 3.45 / 6.018 / 12.55 | 0 |
| native_llama_W1_qwen3-1.7b_start | 95 | 15.458 / 25.7 | 760.6 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 161.8 | 0/95 | 3.55 / 5.943 / 10.3 | 0 |
| cooldown | 6 | 15.75 / 18.7 | 742.8 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 138.4 | 0/6 | 4.04 / 5.57 / 5.57 | 0 |
