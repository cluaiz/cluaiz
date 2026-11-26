| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| deepseek2 30B.A3B Q4_K - Medium | 17.05 GiB | 29.94 B | Full GPU | -1 | 16384 | pp128 | 1235.10 ± 0.00 |
| deepseek2 30B.A3B Q4_K - Medium | 17.05 GiB | 29.94 B | Full GPU | -1 | 16384 | tg32 | 78.55 ± 0.00 |
