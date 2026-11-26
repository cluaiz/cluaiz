| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | GPU + RAM split | 17 | 4096 | pp512 | 1224.93 ± 74.14 |
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | GPU + RAM split | 17 | 4096 | tg128 | 12.88 ± 3.92 |
