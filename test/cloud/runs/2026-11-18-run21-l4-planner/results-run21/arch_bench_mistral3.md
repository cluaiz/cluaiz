| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| mistral3 3B Q4_K - Medium | 1.99 GiB | 3.43 B | Full GPU | -1 | 16384 | pp128 | 4420.72 ± 0.00 |
| mistral3 3B Q4_K - Medium | 1.99 GiB | 3.43 B | Full GPU | -1 | 16384 | tg32 | 96.15 ± 0.00 |
