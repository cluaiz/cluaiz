| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | Full GPU | -1 | 16384 | pp512 | 3613.90 ± 4.79 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | Full GPU | -1 | 16384 | tg128 | 83.05 ± 0.00 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | GPU + RAM split | 24 | 16384 | pp512 | 2234.09 ± 13.79 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | GPU + RAM split | 24 | 16384 | tg128 | 64.78 ± 0.11 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | CPU only | 0 | 16384 | pp512 | 20.70 ± 1.08 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | CPU only | 0 | 16384 | tg128 | 8.94 ± 0.10 |

The planner now loads the fastest of these placements for gpt-oss-20b-gguf-FP4 (planner STEP 4).
