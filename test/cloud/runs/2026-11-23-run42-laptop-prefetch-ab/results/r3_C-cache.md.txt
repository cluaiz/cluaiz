| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| gemma4 26B.A4B IQ4_XS - 4.25 bpw | 13.21 GiB | 25.23 B | MoE split (attention on GPU, experts in RAM) | 30 | 1024 | pp512 | 344.78 ± 43.54 |
| gemma4 26B.A4B IQ4_XS - 4.25 bpw | 13.21 GiB | 25.23 B | MoE split (attention on GPU, experts in RAM) | 30 | 1024 | tg128 | 17.17 ± 0.63 |
