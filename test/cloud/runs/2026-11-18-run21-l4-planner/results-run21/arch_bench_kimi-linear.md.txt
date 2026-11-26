| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| kimi-linear 48B.A3B Q4_K - Medium | 27.99 GiB | 49.12 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | pp128 | 33.75 ± 0.00 |
| kimi-linear 48B.A3B Q4_K - Medium | 27.99 GiB | 49.12 B | MoE split (attention on GPU, experts in RAM) | -1 | 16384 | tg32 | 11.03 ± 0.00 |
