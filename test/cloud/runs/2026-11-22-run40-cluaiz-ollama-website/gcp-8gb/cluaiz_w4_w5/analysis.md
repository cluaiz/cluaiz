# vps26 Analysis

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | embed-gemma | W4 | index+search | 1 | 200/200 | 93.56 | - | - | 2.14 c/s | - | - | top1=0.6471 top5=0.7647 |
| native | cluaiz | embed-gemma | W4 | ramp | 1 | 10/10 | 9.269 | 9.47 | None | None | None | 0 | PASS 2.16 c/s |
| native | cluaiz | embed-gemma | W4 | ramp | 2 | 12/12 | 17.78 | 18.354 | None | None | None | 0 | PASS 2.23 c/s |
| native | cluaiz | embed-gemma | W4 | ramp | 4 | 14/14 | 35.498 | 36.446 | None | None | None | 0 | PASS 2.24 c/s |
| native | cluaiz | speech | W5 | ramp | 1 | 17/17 | 5.097 | 13.026 | None | None | None | 0 | PASS |
| native | cluaiz | speech | W5 | ramp | 2 | 18/18 | 10.535 | 15.518 | None | None | None | 0 | PASS |
| native | cluaiz | speech | W5 | ramp | 4 | 21/21 | 17.139 | 31.122 | None | None | None | 0 | PASS |

## Speech Details
| Mode | Engine | Workload | Task | OK / Total | RTF p50 | RTF p95 | Word Recall |
|---|---|---|---|---|---:|---:|---:|
| native | cluaiz | W5 | tts | 29/29 | 1.19 | 5.499 | None |
| native | cluaiz | W5 | stt_moonshine-tiny | 14/14 | 1.097 | 2.727 | 0.827 |
| native | cluaiz | W5 | stt_moonshine-base | 13/13 | 1.248 | 3.094 | 0.969 |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 13.443 / 21.0 | 750.0 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 160.6 | 0/14 | 3.415 / 6.017 / 6.94 | 0 |
| native_cluaiz_W4_start | 1 | 24.3 / 24.3 | 750.1 | 0.0 | 0.0 | 0.0 | 4.2 / 23.3 | 139.3 | 0/1 | 13.17 / 13.17 / 13.17 | 0 |
| native_cluaiz_W4_index | 94 | 99.229 / 100.0 | 815.0 | 0.0 | 0.0 | 174.588 | 56.7 / 346.3 | 161.8 | 0/94 | 5.985 / 12.345 / 34.14 | 0 |
| native_cluaiz_W4_c1 | 91 | 99.934 / 100.0 | 822.0 | 0.0 | 0.0 | 176.448 | 56.7 / 346.3 | 161.1 | 0/91 | 6.2 / 12.36 / 19.22 | 0 |
| native_cluaiz_W4_c2 | 106 | 98.376 / 100.0 | 826.3 | 0.0 | 0.0 | 168.996 | 57.3 / 346.3 | 182.3 | 0/106 | 5.8 / 9.035 / 18.89 | 0 |
| native_cluaiz_W4_c4 | 126 | 96.325 / 98.6 | 828.0 | 0.0 | 0.0 | 163.377 | 58.1 / 346.3 | 160.5 | 0/126 | 6.395 / 13.255 / 43.32 | 0 |
| native_cluaiz_W5_start | 32 | 60.478 / 69.6 | 1231.5 | 0.0 | 0.0 | 91.153 | 476.2 / 54.1 | 160.0 | 0/32 | 4.595 / 15.219 / 20.92 | 0 |
| native_cluaiz_W5_c1 | 96 | 63.725 / 70.0 | 1864.7 | 0.0 | 0.0 | 99.074 | 1102.6 / 56.3 | 182.4 | 0/96 | 4.61 / 13.407 / 18.84 | 0 |
| native_cluaiz_W5_c2 | 96 | 64.39 / 71.9 | 1990.9 | 0.0 | 0.0 | 99.765 | 1215.5 / 56.3 | 162.5 | 0/96 | 4.455 / 11.43 / 29.23 | 0 |
| native_cluaiz_W5_c4 | 115 | 62.885 / 71.8 | 2145.9 | 0.0 | 0.0 | 97.168 | 1396.4 / 56.3 | 179.9 | 0/115 | 4.69 / 11.238 / 22.65 | 0 |
| cooldown | 6 | 13.55 / 16.4 | 751.4 | 0.0 | 0.0 | 0.0 | 0.0 / 0.0 | 138.8 | 0/6 | 3.35 / 3.507 / 3.55 | 0 |
