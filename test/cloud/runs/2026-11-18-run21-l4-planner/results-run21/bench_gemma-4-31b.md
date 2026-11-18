| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| gemma4 31B Q4_K - Medium | 17.05 GiB | 30.70 B | Full GPU | -1 | 16384 | pp512 | 764.98 ± 8.88 |
| gemma4 31B Q4_K - Medium | 17.05 GiB | 30.70 B | Full GPU | -1 | 16384 | tg128 | 13.01 ± 0.01 |
| gemma4 31B Q4_K - Medium | 17.05 GiB | 30.70 B | GPU + RAM split | 60 | 16384 | pp512 | 703.64 ± 1.64 |
| gemma4 31B Q4_K - Medium | 17.05 GiB | 30.70 B | GPU + RAM split | 60 | 16384 | tg128 | 11.42 ± 0.08 |
| gemma4 31B Q4_K - Medium | 17.05 GiB | 30.70 B | CPU only | 0 | 16384 | pp512 | 6.40 ± 0.00 |
| gemma4 31B Q4_K - Medium | 17.05 GiB | 30.70 B | CPU only | 0 | 16384 | tg128 | 1.59 ± 0.01 |

The planner now loads the fastest of these placements for gemma-4-31b-it-gguf-Q4_K_M (planner STEP 4).
