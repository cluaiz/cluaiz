# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | ollama | qwen3-0.6b | W1 | ramp | 1 | 56/56 | 1.635 | 1.982 | 0.867 | 8.61 | 18.0 | 0 | PASS json=1.0 acc=0.3036 |
| native | ollama | qwen3-0.6b | W1 | ramp | 4 | 69/69 | 5.434 | 6.44 | 4.643 | 10.36 | 18.13 | 0 | PASS json=1.0 acc=0.2899 |
| native | ollama | qwen3-1.7b | W1 | ramp | 1 | 19/19 | 4.515 | 6.398 | 2.539 | 2.77 | 6.98 | 0 | PASS json=1.0 acc=0.7895 |
| native | ollama | qwen3-1.7b | W1 | ramp | 4 | 33/33 | 12.443 | 15.259 | 10.421 | 4.18 | 7.0 | 0 | PASS json=1.0 acc=0.8485 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 18.043 / 34.3 | 681.8 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 137.2 | 0/14 | 4.545 / 9.933 / 11.76 | 0 |
| native_ollama_W1_qwen3-0.6b_start | 1 | 36.1 / 36.1 | 688.1 | 0.0 | 0.0 | 0.0 | 7.0 / 26.5 | 137.2 | 0/1 | 10.02 / 10.02 / 10.02 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c1 | 89 | 97.555 / 100.0 | 1435.7 | 0.0 | 0.0 | 163.817 | 777.1 / 43.1 | 181.5 | 0/89 | 8.96 / 19.288 / 32.84 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c4 | 96 | 97.931 / 100.0 | 1451.9 | 0.0 | 0.0 | 165.029 | 787.6 / 43.1 | 181.9 | 0/96 | 9.23 / 14.87 / 17.59 | 0 |
| native_ollama_W1_qwen3-1.7b_start | 1 | 36.5 / 36.5 | 690.1 | 0.0 | 0.0 | 0.0 | 10.1 / 26.8 | 137.8 | 0/1 | 4.36 / 4.36 / 4.36 | 0 |
| native_ollama_W1_qwen3-1.7b_ramp_c1 | 89 | 94.457 / 100.0 | 2064.5 | 0.0 | 0.0 | 157.508 | 1424.7 / 43.8 | 160.8 | 0/89 | 8.9 / 19.386 / 28.44 | 0 |
| native_ollama_W1_qwen3-1.7b_ramp_c4 | 101 | 98.386 / 100.0 | 2085.7 | 0.0 | 0.0 | 165.527 | 1439.6 / 43.8 | 182.1 | 0/101 | 9.47 / 17.92 / 30.5 | 0 |
| cooldown | 6 | 15.633 / 19.5 | 671.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 181.5 | 0/6 | 4.635 / 4.947 / 4.95 | 0 |
