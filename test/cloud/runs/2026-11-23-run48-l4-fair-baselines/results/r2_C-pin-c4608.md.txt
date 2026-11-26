| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 4608 | pp512 | 37.95 ± 13.15 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 4608 | pp2048 | 76.19 ± 0.07 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 4608 | pp4096 | 66.58 ± 0.73 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 4608 | tg128 | 10.60 ± 0.44 |
