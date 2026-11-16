| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | pp512 | 1072.03 ± 315.13 |
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | tg128 | 77.39 ± 4.51 |
