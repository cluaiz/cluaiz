| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | Full GPU | -1 | 16384 | pp512 | 3680.17 ± 0.00 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | Full GPU | -1 | 16384 | tg128 | 83.32 ± 0.00 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | GPU + RAM split | 24 | 16384 | pp512 | 2231.65 ± 0.00 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | GPU + RAM split | 24 | 16384 | tg128 | 64.85 ± 0.00 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | CPU only | 0 | 16384 | pp512 | 21.54 ± 0.00 |
| gpt-oss 20B MXFP4 MoE | 11.27 GiB | 20.91 B | CPU only | 0 | 16384 | tg128 | 9.59 ± 0.00 |

The planner now loads the fastest of these placements for gpt-oss-20b-MXFP4.gguf (planner STEP 4).
