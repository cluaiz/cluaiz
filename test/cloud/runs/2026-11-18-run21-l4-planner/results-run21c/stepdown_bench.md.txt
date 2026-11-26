| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen3moe 30B.A3B Q4_K - Medium | 17.28 GiB | 30.53 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp64 | 33.02 ± 0.00 |
| qwen3moe 30B.A3B Q4_K - Medium | 17.28 GiB | 30.53 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | tg16 | 12.02 ± 0.00 |
