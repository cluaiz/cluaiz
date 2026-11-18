| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | Full GPU | -1 | 16384 | pp512 | 2042.49 ± 0.00 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | Full GPU | -1 | 16384 | tg128 | 31.74 ± 0.00 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | GPU + RAM split | 48 | 16384 | pp512 | 1837.91 ± 0.00 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | GPU + RAM split | 48 | 16384 | tg128 | 27.76 ± 0.00 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | CPU only | 0 | 16384 | pp512 | 17.34 ± 0.00 |
| gemma3 12B Q4_K - Medium | 6.79 GiB | 11.77 B | CPU only | 0 | 16384 | tg128 | 3.94 ± 0.00 |

The planner now loads the fastest of these placements for gemma-3-12b-it-gguf-Q4_K_M (planner STEP 4).
