| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | GPU + RAM split | 9 | 4096 | pp512 | 1266.36 ± 307.69 |
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | GPU + RAM split | 9 | 4096 | tg128 | 67.88 ± 0.21 |
