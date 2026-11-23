| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw | 1.66 GiB | 3.30 B | MoE split (attention on GPU, experts in RAM) | -1 | 1024 | pp512 | 1515.50 ± 77.14 |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw | 1.66 GiB | 3.30 B | MoE split (attention on GPU, experts in RAM) | -1 | 1024 | tg128 | 38.35 ± 1.83 |
