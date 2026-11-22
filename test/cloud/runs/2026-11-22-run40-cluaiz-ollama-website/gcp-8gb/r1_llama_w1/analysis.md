# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 14.25 / 21.5 | 739.9 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 136.7 | 0/14 | 4.015 / 8.941 / 10.39 | 0 |
| native_llama_W1_qwen3-0.6b_start | 95 | 14.88 / 25.1 | 749.8 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 159.5 | 0/95 | 3.59 / 5.468 / 10.4 | 0 |
| native_llama_W1_qwen3-1.7b_start | 95 | 14.631 / 25.2 | 751.1 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 159.8 | 0/95 | 3.5 / 7.535 / 15.08 | 0 |
| cooldown | 6 | 14.967 / 19.7 | 746.6 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 138.3 | 0/6 | 3.33 / 3.71 / 3.71 | 0 |
