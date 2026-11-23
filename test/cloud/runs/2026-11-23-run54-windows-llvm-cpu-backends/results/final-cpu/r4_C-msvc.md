| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen3 1.7B IQ4_XS - 4.25 bpw | 1.09 GiB | 2.03 B | CPU only, experts read from disk | 0 | 1024 | pp512 | 73.25 ± 14.68 |
| qwen3 1.7B IQ4_XS - 4.25 bpw | 1.09 GiB | 2.03 B | CPU only, experts read from disk | 0 | 1024 | tg128 | 18.61 ± 1.49 |
