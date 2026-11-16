| model                          |       size |     params | backend    | threads | type_k | type_v |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | -----: | -----: | --------------: | -------------------: |
| llama 3B Q4_K - Medium         |   1.87 GiB |     3.21 B | CPU        |       1 |   q8_0 |   q8_0 |           pp512 |         16.71 ± 0.16 |
| llama 3B Q4_K - Medium         |   1.87 GiB |     3.21 B | CPU        |       1 |   q8_0 |   q8_0 |           tg128 |          7.73 ± 0.06 |

build: b4c0e00a4 (12428)
