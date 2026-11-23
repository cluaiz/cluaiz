| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp512 | 44.33 ± 1.18 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp2048 | 85.91 ± 0.32 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp4096 | 92.59 ± 1.03 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | tg128 | 10.82 ± 0.09 |
