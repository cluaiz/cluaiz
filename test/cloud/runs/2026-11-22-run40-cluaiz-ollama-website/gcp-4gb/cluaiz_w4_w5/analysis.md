# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | embed-gemma | W4 | index+search | 1 | 200/200 | 106.15 | - | - | 1.88 c/s | - | - | top1=0.6471 top5=0.7647 |
| native | cluaiz | embed-gemma | W4 | ramp | 1 | 9/9 | 10.247 | 11.385 | None | None | None | 0 | PASS 1.91 c/s |
| native | cluaiz | embed-gemma | W4 | ramp | 2 | 11/11 | 19.51 | 20.457 | None | None | None | 0 | PASS 2.02 c/s |
| native | cluaiz | embed-gemma | W4 | ramp | 4 | 13/13 | 38.386 | 38.773 | None | None | None | 0 | PASS 2.07 c/s |
| native | cluaiz | speech | W5 | ramp | 1 | 15/15 | 6.033 | 15.422 | None | None | None | 0 | PASS |
| native | cluaiz | speech | W5 | ramp | 2 | 16/16 | 12.234 | 18.81 | None | None | None | 0 | PASS |
| native | cluaiz | speech | W5 | ramp | 4 | 19/19 | 21.357 | 37.23 | None | None | None | 0 | PASS |

## Speech Details
| Mode | Engine | Workload | Task | OK / Total | RTF p50 | RTF p95 | Word Recall |
|---|---|---|---|---|---:|---:|---:|
| native | cluaiz | W5 | tts | 26/26 | 1.423 | 6.828 | None |
| native | cluaiz | W5 | stt_moonshine-tiny | 13/13 | 1.292 | 3.124 | 0.839 |
| native | cluaiz | W5 | stt_moonshine-base | 11/11 | 1.313 | 2.719 | 0.973 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 16.657 / 24.8 | 655.7 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 158.9 | 0/14 | 4.345 / 5.807 / 5.82 | 0 |
| native_cluaiz_W4_start | 1 | 32.0 / 32.0 | 653.7 | 0.0 | 0.0 | 0.0 | 4.2 / 23.1 | 136.6 | 0/1 | 16.94 / 16.94 / 16.94 | 0 |
| native_cluaiz_W4_index | 107 | 99.229 / 100.0 | 718.8 | 0.0 | 0.0 | 166.513 | 56.8 / 349.2 | 182.1 | 0/107 | 8.97 / 18.897 / 32.46 | 0 |
| native_cluaiz_W4_c1 | 91 | 99.934 / 100.0 | 719.1 | 0.0 | 0.0 | 169.847 | 56.8 / 349.2 | 160.5 | 0/91 | 8.28 / 22.33 / 69.22 | 0 |
| native_cluaiz_W4_c2 | 106 | 98.916 / 100.0 | 720.9 | 0.0 | 0.0 | 166.458 | 57.4 / 349.3 | 161.4 | 0/106 | 8.295 / 14.895 / 20.3 | 0 |
| native_cluaiz_W4_c4 | 127 | 96.259 / 98.6 | 724.8 | 0.0 | 0.0 | 157.597 | 57.8 / 349.3 | 160.2 | 0/127 | 7.74 / 15.159 / 23.21 | 0 |
| native_cluaiz_W5_start | 37 | 65.311 / 73.8 | 1120.2 | 0.0 | 0.0 | 93.851 | 472.2 / 54.9 | 160.4 | 0/37 | 6.26 / 16.576 / 28.0 | 0 |
| native_cluaiz_W5_c1 | 97 | 67.571 / 96.3 | 1758.2 | 0.0 | 0.0 | 98.58 | 1118.2 / 57.1 | 160.5 | 0/97 | 6.42 / 18.342 / 23.93 | 0 |
| native_cluaiz_W5_c2 | 97 | 67.377 / 78.2 | 1837.0 | 0.0 | 0.0 | 99.261 | 1188.4 / 57.1 | 160.2 | 0/97 | 6.67 / 19.28 / 24.74 | 0 |
| native_cluaiz_W5_c4 | 124 | 66.394 / 95.9 | 2089.2 | 0.0 | 0.0 | 96.51 | 1451.8 / 56.0 | 169.2 | 0/124 | 6.225 / 18.235 / 51.7 | 0 |
| cooldown | 6 | 17.117 / 26.9 | 670.3 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 148.2 | 0/6 | 4.11 / 10.203 / 10.26 | 0 |
