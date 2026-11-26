| model                          |       size |     params | backend    | ngl |  n_cpu_moe |  fa |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --: | --------------: | -------------------: |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw |   1.66 GiB |     3.30 B | CUDA       |  99 |         32 |   1 |           pp512 |     1322.21 ± 108.52 |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw |   1.66 GiB |     3.30 B | CUDA       |  99 |         32 |   1 |           tg128 |         30.66 ± 8.25 |

build: b4c0e00a4 (12428)
