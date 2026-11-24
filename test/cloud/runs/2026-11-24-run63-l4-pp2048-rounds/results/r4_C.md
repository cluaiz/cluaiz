| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp512 | 48.81 ± 0.98 |
| qwen35moe 35B.A3B IQ4_XS - 4.25 bpw | 16.95 GiB | 35.51 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp2048 | 88.17 ± 0.11 |
