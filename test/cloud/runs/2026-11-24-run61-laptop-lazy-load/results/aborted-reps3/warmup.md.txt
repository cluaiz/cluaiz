| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| gemma4 26B.A4B IQ4_XS - 4.25 bpw | 13.21 GiB | 25.23 B | MoE split (attention on GPU, experts in RAM) | -1 | 1024 | tg8 | 1.61 ± 0.00 |
