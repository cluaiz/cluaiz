| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | GPU + RAM split | 17 | 4096 | pp512 | 1310.21 ± 87.60 |
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | GPU + RAM split | 17 | 4096 | tg128 | 17.72 ± 4.61 |
