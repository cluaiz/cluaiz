# vps26 Analysis

## Docker ON vs OFF Baseline
- Docker ON machine used avg: 478.21 MB (Docker daemon RSS avg: 102.08 MB)
- Docker OFF machine used avg: 422.98 MB
- Delta machine used memory: 55.23 MB

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-0.6b | W1 | ramp | 1 | 37/37 | 0.626 | 0.758 | 0.341 | 14.58 | 35.43 | 0 | PASS json=1.0 acc=0.3243 |
| native | cluaiz | qwen3-0.6b | W1 | ramp | 2 | 39/39 | 1.281 | 1.937 | 1.0 | 15.01 | 34.34 | 0 | PASS json=1.0 acc=0.3333 |
| native | cluaiz | embed-gemma | W4 | index+search | 1 | 100/100 | 20.94 | - | - | 4.78 c/s | - | - | top1=0.875 top5=0.875 |
| native | cluaiz | embed-gemma | W4 | ramp | 1 | 7/7 | 3.976 | 4.406 | None | None | None | 0 | PASS 4.95 c/s |
| native | cluaiz | embed-gemma | W4 | ramp | 2 | 3/8163 | 8.866 | 9.282 | None | None | None | 0 | BREAK: error_rate=99.96% |
| native | cluaiz | speech | W5 | ramp | 1 | 9/9 | 2.61 | 6.119 | None | None | None | 0 | PASS |
| native | cluaiz | speech | W5 | ramp | 2 | 10/10 | 5.264 | 7.86 | None | None | None | 0 | PASS |
| native | cluaiz | qwen3-0.6b | W6 | ramp | 1 | 18/18 | 3.247 | 11.084 | 1.321 | 3.75 | 11.2 | 0 | PASS |
| native | cluaiz | qwen3-0.6b | W6 | ramp | 2 | 26/26 | 2.933 | 8.032 | 2.022 | 6.81 | 12.92 | 0 | PASS |
| native | llama | qwen3-0.6b | W1 | ramp | 1 | 37/37 | 0.722 | 0.834 | 0.413 | 21.98 | 47.28 | 0 | PASS json=1.0 acc=0.3243 |
| native | llama | qwen3-0.6b | W1 | ramp | 2 | 47/47 | 1.121 | 1.416 | 0.823 | 27.27 | 48.64 | 0 | PASS json=1.0 acc=0.3404 |
| native | llama | embed-gemma | W4 | index+search | 1 | 100/100 | 19.78 | - | - | 5.05 c/s | - | - | top1=0.875 top5=0.875 |
| native | llama | embed-gemma | W4 | ramp | 1 | 6/6 | 4.184 | 4.637 | None | None | None | 0 | PASS 4.69 c/s |
| native | llama | embed-gemma | W4 | ramp | 2 | 8/8 | 7.816 | 7.849 | None | None | None | 0 | PASS 5.12 c/s |
| native | ollama | qwen3-0.6b | W1 | ramp | 1 | 36/36 | 0.702 | 0.797 | 0.398 | 20.97 | 47.88 | 0 | PASS json=1.0 acc=0.3333 |
| native | ollama | qwen3-0.6b | W1 | ramp | 2 | 46/46 | 1.135 | 1.45 | 0.83 | 26.26 | 47.4 | 0 | PASS json=1.0 acc=0.3478 |
| native | ollama | embed-gemma | W4 | index+search | 1 | 100/100 | 20.91 | - | - | 4.78 c/s | - | - | top1=0.75 top5=0.875 |
| native | ollama | embed-gemma | W4 | ramp | 1 | 7/7 | 3.94 | 4.414 | None | None | None | 0 | PASS 4.99 c/s |
| native | ollama | embed-gemma | W4 | ramp | 2 | 8/8 | 7.956 | 8.21 | None | None | None | 0 | PASS 4.99 c/s |

## Speech Details
| Mode | Engine | Workload | Task | OK / Total | RTF p50 | RTF p95 | Word Recall |
|---|---|---|---|---|---:|---:|---:|
| native | cluaiz | W5 | tts | 10/10 | 0.548 | 0.682 | None |
| native | cluaiz | W5 | stt_moonshine-tiny | 5/5 | 0.587 | 0.627 | 0.727 |
| native | cluaiz | W5 | stt_moonshine-base | 4/4 | 0.421 | 0.707 | 0.95 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 28 | 2.821 / 9.5 | 601.1 | 0.107 | 55.2 | 0.0 | 0.0 / 0.0 | 98.8 | 0/28 | 8.99 / 16.821 / 45.5 | 0 |
| native_cluaiz_W1_qwen3-0.6b_start | 2 | 36.5 / 52.0 | 608.8 | 0.0 | 55.1 | 0.0 | 8.0 / 31.6 | 87.8 | 0/2 | 13.075 / 16.742 / 17.15 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c1 | 25 | 98.848 / 100.0 | 1160.5 | 0.12 | 55.1 | 94.728 | 560.3 / 411.3 | 87.9 | 0/25 | 15.11 / 17.434 / 17.97 | 0 |
| native_cluaiz_W1_qwen3-0.6b_ramp_c2 | 28 | 92.404 / 100.0 | 1187.6 | 0.143 | 54.9 | 86.789 | 587.7 / 411.3 | 87.9 | 0/28 | 15.85 / 26.385 / 32.38 | 0 |
| native_cluaiz_W4_start | 1 | 20.0 / 20.0 | 649.3 | 0.0 | 54.9 | 0.0 | 0.8 / 6.2 | 87.9 | 0/1 | 7.9 / 7.9 / 7.9 | 0 |
| native_cluaiz_W4_index | 21 | 97.257 / 100.0 | 692.3 | 0.048 | 54.9 | 94.086 | 56.1 / 348.2 | 106.7 | 0/21 | 14.79 / 17.22 / 20.59 | 0 |
| native_cluaiz_W4_c1 | 28 | 100.0 / 100.0 | 660.6 | 0.107 | 54.9 | 97.604 | 56.2 / 348.2 | 88.0 | 0/28 | 14.695 / 16.867 / 33.67 | 0 |
| native_cluaiz_W4_c2 | 29 | 91.414 / 100.0 | 673.5 | 0.069 | 54.2 | 73.297 | 56.5 / 348.2 | 107.7 | 0/29 | 17.08 / 23.388 / 41.99 | 0 |
| native_cluaiz_W5_start | 17 | 91.494 / 100.0 | 1085.0 | 0.176 | 54.3 | 86.6 | 472.1 / 51.9 | 88.1 | 0/17 | 14.02 / 16.888 / 17.92 | 0 |
| native_cluaiz_W5_c1 | 26 | 99.654 / 100.0 | 1697.5 | 0.077 | 52.7 | 96.723 | 1089.7 / 53.3 | 88.1 | 0/26 | 15.24 / 20.148 / 24.17 | 0 |
| native_cluaiz_W5_c2 | 29 | 91.683 / 100.0 | 1832.2 | 0.069 | 52.7 | 87.452 | 1222.1 / 53.3 | 88.1 | 0/29 | 14.29 / 16.302 / 17.22 | 0 |
| native_cluaiz_W6_start | 1 | 26.5 / 26.5 | 638.9 | 0.0 | 52.7 | 0.0 | 7.8 / 31.2 | 88.1 | 0/1 | 7.49 / 7.49 / 7.49 | 0 |
| native_cluaiz_W6_ramp_c1 | 29 | 99.207 / 100.0 | 1838.7 | 0.103 | 52.7 | 95.724 | 1235.1 / 741.4 | 88.1 | 0/29 | 16.58 / 22.674 / 23.98 | 0 |
| native_cluaiz_W6_ramp_c2 | 29 | 91.483 / 100.0 | 2020.7 | 0.138 | 51.5 | 87.062 | 1416.1 / 741.4 | 88.2 | 0/29 | 15.03 / 17.01 / 20.91 | 0 |
| cooldown | 12 | 2.8 / 7.7 | 631.8 | 0.083 | 51.5 | 0.0 | 0.0 / 0.0 | 88.2 | 0/12 | 9.24 / 22.518 / 25.73 | 0 |
| native_ollama_W1_qwen3-0.6b_start | 1 | 21.6 / 21.6 | 567.8 | 1.0 | 55.2 | 0.0 | 4.0 / 11.5 | 79.3 | 0/1 | 19.3 / 19.3 / 19.3 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c1 | 25 | 98.112 / 100.0 | 1358.9 | 0.04 | 55.2 | 92.304 | 766.6 / 43.4 | 79.3 | 0/25 | 14.98 / 18.11 / 18.56 | 0 |
| native_ollama_W1_qwen3-0.6b_ramp_c2 | 29 | 92.631 / 100.0 | 1359.1 | 0.138 | 55.2 | 87.372 | 766.8 / 43.4 | 79.3 | 0/29 | 14.99 / 20.394 / 29.16 | 0 |
| native_ollama_W4_start | 1 | 21.8 / 21.8 | 631.5 | 1.0 | 55.3 | 0.0 | 0.0 / 0.0 | 79.3 | 0/1 | 7.14 / 7.14 / 7.14 | 0 |
| native_ollama_W4_index | 22 | 97.791 / 100.0 | 980.6 | 0.0 | 55.3 | 92.114 | 381.8 / 43.2 | 98.6 | 0/22 | 13.99 / 22.36 / 40.57 | 0 |
| native_ollama_W4_c1 | 27 | 100.0 / 100.0 | 967.4 | 0.111 | 54.0 | 97.667 | 382.0 / 43.2 | 79.3 | 0/27 | 14.64 / 19.37 / 34.23 | 0 |
| native_ollama_W4_c2 | 35 | 93.126 / 100.0 | 968.1 | 0.086 | 52.8 | 89.457 | 383.1 / 43.2 | 98.1 | 0/35 | 14.29 / 19.785 / 35.75 | 0 |
| native_llama_W1_qwen3-0.6b_start | 2 | 64.6 / 82.7 | 801.3 | 0.5 | 52.8 | 39.0 | 234.1 / 394.5 | 79.3 | 0/2 | 18.715 / 20.916 / 21.16 | 0 |
| native_llama_W1_qwen3-0.6b_ramp_c1 | 25 | 98.864 / 100.0 | 1167.4 | 0.08 | 52.8 | 95.14 | 589.1 / 389.5 | 79.3 | 0/25 | 15.1 / 50.528 / 92.62 | 0 |
| native_llama_W1_qwen3-0.6b_ramp_c2 | 29 | 90.448 / 100.0 | 1192.1 | 0.103 | 52.8 | 86.952 | 589.2 / 389.5 | 79.3 | 0/29 | 15.17 / 17.39 / 21.09 | 0 |
| native_llama_W4_start | 2 | 62.05 / 85.1 | 666.0 | 0.0 | 52.3 | 12.8 | 53.9 / 329.7 | 79.3 | 0/2 | 15.125 / 18.136 / 18.47 | 0 |
| native_llama_W4_index | 20 | 100.0 / 100.0 | 649.2 | 0.15 | 52.3 | 97.245 | 62.7 / 329.9 | 99.3 | 0/20 | 16.02 / 20.452 / 24.47 | 0 |
| native_llama_W4_c1 | 25 | 100.0 / 100.0 | 648.4 | 0.12 | 51.5 | 97.216 | 62.7 / 329.9 | 79.3 | 0/25 | 16.05 / 25.056 / 35.4 | 0 |
| native_llama_W4_c2 | 34 | 92.488 / 100.0 | 647.3 | 0.029 | 51.5 | 89.197 | 62.8 / 329.9 | 79.4 | 0/34 | 14.82 / 16.956 / 17.23 | 0 |
