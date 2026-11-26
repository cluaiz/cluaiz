| model                          |       size |     params | backend    | ngl |  n_cpu_moe |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --------------: | -------------------: |
| olmoe A1.7B Q4_K - Medium      |   3.92 GiB |     6.92 B | CUDA       |  99 |         16 |           pp512 |      1071.12 ± 34.93 |
| olmoe A1.7B Q4_K - Medium      |   3.92 GiB |     6.92 B | CUDA       |  99 |         16 |           tg128 |         46.67 ± 3.38 |

build: b4c0e00a4 (12428)
