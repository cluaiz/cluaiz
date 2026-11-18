| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| llama 13B Q4_K - Medium | 13.34 GiB | 23.57 B | Full GPU | -1 | 16384 | pp512 | 1065.59 ± 2.38 |
| llama 13B Q4_K - Medium | 13.34 GiB | 23.57 B | Full GPU | -1 | 16384 | tg128 | 17.50 ± 0.00 |
| llama 13B Q4_K - Medium | 13.34 GiB | 23.57 B | GPU + RAM split | 40 | 16384 | pp512 | 934.37 ± 3.34 |
| llama 13B Q4_K - Medium | 13.34 GiB | 23.57 B | GPU + RAM split | 40 | 16384 | tg128 | 14.72 ± 0.07 |
| llama 13B Q4_K - Medium | 13.34 GiB | 23.57 B | CPU only | 0 | 16384 | pp512 | 8.81 ± 0.00 |
| llama 13B Q4_K - Medium | 13.34 GiB | 23.57 B | CPU only | 0 | 16384 | tg128 | 2.19 ± 0.00 |

The planner now loads the fastest of these placements for mistral-small-3.2-24b-instruct-2506-gguf-Q4_K_M (planner STEP 4).
