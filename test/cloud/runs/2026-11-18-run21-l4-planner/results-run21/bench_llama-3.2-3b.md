| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | Full GPU | -1 | 16384 | pp512 | 7592.76 ± 274.43 |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | Full GPU | -1 | 16384 | tg128 | 109.58 ± 0.16 |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | GPU + RAM split | 28 | 16384 | pp512 | 6279.03 ± 25.43 |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | GPU + RAM split | 28 | 16384 | tg128 | 88.68 ± 0.10 |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | CPU only | 0 | 16384 | pp512 | 64.74 ± 0.03 |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | CPU only | 0 | 16384 | tg128 | 13.23 ± 0.04 |

The planner now loads the fastest of these placements for llama-3.2-3b-instruct-gguf-Q4_K_M (planner STEP 4).
