| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | Full GPU | -1 | 16384 | pp512 | 2013.62 ± 28.66 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | Full GPU | -1 | 16384 | tg128 | 31.68 ± 0.01 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | GPU + RAM split | 48 | 16384 | pp512 | 1817.95 ± 7.57 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | GPU + RAM split | 48 | 16384 | tg128 | 27.63 ± 0.07 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | CPU only | 0 | 16384 | pp512 | 17.34 ± 0.02 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | CPU only | 0 | 16384 | tg128 | 3.85 ± 0.03 |

The planner now loads the fastest of these placements for gemma-3-12b-it-gguf-Q4_K_M (planner STEP 4).
