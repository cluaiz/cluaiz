| model                          |       size |     params | backend    | ngl |  n_cpu_moe |  fa |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --: | --------------: | -------------------: |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw |   1.66 GiB |     3.30 B | CUDA       |  99 |         32 |   1 |           pp512 |     1352.44 ± 140.85 |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw |   1.66 GiB |     3.30 B | CUDA       |  99 |         32 |   1 |           tg128 |         27.99 ± 9.77 |

build: b4c0e00a4 (12428)
