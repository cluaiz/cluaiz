| model                          |       size |     params | backend    | ngl |  n_cpu_moe |  fa |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --: | --------------: | -------------------: |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw |   1.66 GiB |     3.30 B | CUDA       |  99 |         32 |   1 |           pp512 |      1626.25 ± 45.41 |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw |   1.66 GiB |     3.30 B | CUDA       |  99 |         32 |   1 |           tg128 |         24.80 ± 3.85 |

build: b4c0e00a4 (12428)
