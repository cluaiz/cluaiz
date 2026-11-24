| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp512 | 50.77 ± 0.78 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp2048 | 91.81 ± 4.88 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | tg128 | 12.51 ± 0.31 |
