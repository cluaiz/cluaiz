| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen3moe 30B.A3B Q4_K - Medium | 17.28 GiB | 30.53 B | Full GPU | -1 | 16384 | pp512 | 3274.78 ± 28.82 |
| qwen3moe 30B.A3B Q4_K - Medium | 17.28 GiB | 30.53 B | Full GPU | -1 | 16384 | tg128 | 102.82 ± 0.07 |
| qwen3moe 30B.A3B Q4_K - Medium | 17.28 GiB | 30.53 B | GPU + RAM split | 48 | 16384 | pp512 | 2253.74 ± 24.70 |
| qwen3moe 30B.A3B Q4_K - Medium | 17.28 GiB | 30.53 B | GPU + RAM split | 48 | 16384 | tg128 | 89.89 ± 0.25 |
| qwen3moe 30B.A3B Q4_K - Medium | 17.28 GiB | 30.53 B | CPU only | 0 | 16384 | pp512 | 34.26 ± 0.31 |
| qwen3moe 30B.A3B Q4_K - Medium | 17.28 GiB | 30.53 B | CPU only | 0 | 16384 | tg128 | 12.16 ± 0.01 |

The planner now loads the fastest of these placements for qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M (planner STEP 4).
