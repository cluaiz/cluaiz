# vps26 Analysis

## Docker ON vs OFF Baseline
- Docker ON machine used avg: 478.21 MB (Docker daemon RSS avg: 102.08 MB)
- Docker OFF machine used avg: 422.98 MB
- Delta machine used memory: 55.23 MB

## Workload Steps
| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |
|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| native | cluaiz | qwen3-0.6b | W6 | soak | 16 | 1063/1063 | 79.719 | 93.998 | 81.69 | 1.89 | 4.88 | 0 | BREAK: p95=93.998s > 60s |

## Per-Phase Resource & Neighbour App Table
| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|
| baseline_with_neighbour | 14 | 11.493 / 66.3 | 521.5 | 0.0 | 55.2 | 0.0 | 0.0 / 0.0 | 94.2 | 0/14 | 9.125 / 19.618 / 28.66 | 0 |
| native_cluaiz_W6_start | 2 | 46.65 / 62.3 | 512.4 | 0.0 | 55.2 | 0.0 | 7.8 / 32.0 | 76.1 | 0/2 | 18.28 / 20.044 / 20.24 | 0 |
| native_cluaiz_W6_soak_c16 | 3470 | 99.923 / 100.0 | 2433.9 | 0.121 | 55.3 | 94.78 | 1714.7 / 742.2 | 100.8 | 0/3470 | 16.95 / 174.2 / 505.39 | 0 |
| cooldown | 6 | 2.8 / 3.9 | 549.0 | 0.0 | 50.1 | 0.0 | 0.0 / 0.0 | 76.2 | 0/6 | 10.625 / 15.892 / 16.99 | 0 |
