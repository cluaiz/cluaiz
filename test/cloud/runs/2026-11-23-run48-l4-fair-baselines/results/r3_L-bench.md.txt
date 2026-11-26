| model                          |       size |     params | backend    | ngl |  n_cpu_moe |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --------------: | -------------------: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           pp512 |         50.10 ± 0.02 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp2048 |        106.95 ± 7.05 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp4096 |         86.74 ± 8.61 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           tg128 |         10.49 ± 0.18 |

build: b54752d (1)
