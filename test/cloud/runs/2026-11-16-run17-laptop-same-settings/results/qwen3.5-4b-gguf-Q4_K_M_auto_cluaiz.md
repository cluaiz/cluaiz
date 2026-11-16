| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | GPU + RAM split | 15 | 8192 | pp512 | 1139.26 ± 58.01 |
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | GPU + RAM split | 15 | 8192 | tg128 | 10.89 ± 3.34 |
