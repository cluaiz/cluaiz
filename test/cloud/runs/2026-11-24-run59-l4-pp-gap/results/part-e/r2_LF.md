| model                          |       size |     params | backend    | ngl |  n_cpu_moe |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | --: | ---------: | --------------: | -------------------: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           pp512 |         51.13 ± 0.02 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |          pp2048 |         99.59 ± 0.95 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw |  16.95 GiB |    35.51 B | CUDA       |  99 |         41 |           tg128 |         13.04 ± 0.20 |

build: 8676a916e (12420)
