| model                          |       size |     params | backend    | ngl |  n_cpu_moe |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --------------: | -------------------: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           pp512 |         48.73 ± 0.24 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp2048 |         97.65 ± 4.24 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp4096 |        101.34 ± 2.96 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           tg128 |         11.14 ± 0.80 |

build: 8676a916e (12420)
