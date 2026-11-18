| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| lfm2 1.2B Q4_K - Medium | 0.74 GiB | 1.17 B | Full GPU | -1 | 16384 | pp128 | 9324.58 ± 0.00 |
| lfm2 1.2B Q4_K - Medium | 0.74 GiB | 1.17 B | Full GPU | -1 | 16384 | tg32 | 266.07 ± 0.00 |
