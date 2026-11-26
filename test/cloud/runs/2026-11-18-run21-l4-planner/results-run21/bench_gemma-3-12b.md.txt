| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | Full GPU | -1 | 16384 | pp512 | 2030.71 ± 15.16 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | Full GPU | -1 | 16384 | tg128 | 31.74 ± 0.01 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | GPU + RAM split | 48 | 16384 | pp512 | 1847.42 ± 2.39 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | GPU + RAM split | 48 | 16384 | tg128 | 27.64 ± 0.03 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | CPU only | 0 | 16384 | pp512 | 17.32 ± 0.02 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | CPU only | 0 | 16384 | tg128 | 3.91 ± 0.01 |

The planner now loads the fastest of these placements for gemma-3-12b-it-gguf-Q4_K_M (planner STEP 4).
