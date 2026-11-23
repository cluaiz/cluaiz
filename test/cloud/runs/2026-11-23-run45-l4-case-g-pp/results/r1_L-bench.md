| model                          |       size |     params | backend    | ngl |  n_cpu_moe |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --------------: | -------------------: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           pp512 |         52.03 ± 0.11 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp2048 |        108.21 ± 0.31 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp4096 |        89.36 ± 11.29 |

build: b54752d (1)
