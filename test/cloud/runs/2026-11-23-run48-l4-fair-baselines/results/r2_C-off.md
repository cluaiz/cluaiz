| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp512 | 111.22 ± 1.81 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp2048 | 210.40 ± 2.40 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp4096 | 239.35 ± 10.38 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | tg128 | 16.91 ± 0.11 |
