| model                          |       size |     params | backend    | ngl |  n_cpu_moe |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --------------: | -------------------: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           pp512 |         49.37 ± 0.69 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp2048 |        108.14 ± 1.34 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp4096 |         84.61 ± 1.57 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           tg128 |         11.43 ± 0.42 |

build: 8676a916e (12420)
