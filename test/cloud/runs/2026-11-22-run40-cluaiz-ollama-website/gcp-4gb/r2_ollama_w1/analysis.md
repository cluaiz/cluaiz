# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | ollama | qwen3-0.6b | W1 | ramp | 1 | 56/56 | 1.587 | 2.135 | 0.887 | 8.59 | 18.04 | 0 | PASS json=1.0 acc=0.3036 |
| native | ollama | qwen3-0.6b | W1 | ramp | 4 | 64/64 | 5.823 | 7.509 | 4.993 | 9.53 | 16.58 | 0 | PASS json=1.0 acc=0.2812 |
| native | ollama | qwen3-1.7b | W1 | ramp | 1 | 14/14 | 5.854 | 12.202 | 3.337 | 1.97 | 5.29 | 0 | PASS json=1.0 acc=0.8571 |
| native | ollama | qwen3-1.7b | W1 | ramp | 4 | 29/29 | 13.417 | 17.686 | 11.881 | 3.52 | 6.26 | 0 | PASS json=1.0 acc=0.8276 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 15.729 / 24.3 | 661.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 159.7 | 0/14 | 4.555 / 6.454 / 7.93 | 0 |
| native_ollama_W1_qwen3-0.6b_start | 1 | 30.2 / 30.2 | 674.2 | 0.0 | 0.0 | 0.0 | 10.1 / 27.0 | 158.3 | 0/1 | 6.95 / 6.95 / 6.95 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c1 | 89 | 97.36 / 100.0 | 1451.0 | 0.0 | 0.0 | 163.93 | 777.2 / 43.6 | 182.7 | 0/89 | 8.98 / 20.124 / 36.35 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c4 | 97 | 97.622 / 100.0 | 1455.2 | 0.0 | 0.0 | 160.903 | 782.3 / 43.6 | 161.4 | 0/97 | 9.84 / 19.55 / 28.41 | 0 |
| native_ollama_W1_qwen3-1.7b_start | 1 | 58.5 / 58.5 | 693.0 | 0.0 | 0.0 | 0.0 | 10.4 / 27.8 | 159.2 | 0/1 | 5.28 / 5.28 / 5.28 | 0 |
| native_ollama_W1_qwen3-1.7b_ramp_c1 | 91 | 95.115 / 100.0 | 2061.1 | 0.0 | 0.0 | 147.444 | 1419.9 / 44.3 | 181.2 | 0/91 | 14.29 / 35.43 / 45.37 | 0 |
| native_ollama_W1_qwen3-1.7b_ramp_c4 | 104 | 97.556 / 100.0 | 2081.2 | 0.0 | 0.0 | 159.449 | 1444.4 / 44.3 | 181.2 | 0/104 | 11.215 / 21.159 / 28.85 | 0 |
| cooldown | 6 | 17.25 / 22.9 | 646.6 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 158.8 | 0/6 | 4.315 / 8.55 / 8.55 | 0 |
