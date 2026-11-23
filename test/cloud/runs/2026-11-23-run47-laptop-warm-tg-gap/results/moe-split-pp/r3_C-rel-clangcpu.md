| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw | 1.66 GiB | 3.30 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | pp512 | 1545.64 ± 49.66 |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw | 1.66 GiB | 3.30 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | pp2048 | 1521.53 ± 53.46 |
| granitemoe 3B.A800M IQ4_XS - 4.25 bpw | 1.66 GiB | 3.30 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | tg128 | 30.34 ± 3.18 |
