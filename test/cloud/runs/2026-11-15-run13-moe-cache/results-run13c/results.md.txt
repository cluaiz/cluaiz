| Setting | Gen t/s (avg, min-max) | GPU MiB | fit / cache in the log |
|---|---|---|---|
| gptoss20-4096-mlock-cmoe+cache | 21.57 (20.96-22.24) | 3534 | MoE cache size =  1588.46 MiB for 9682.03 MiB of host experts, 80% of the misses on th; hit rate = 15.14% |
| gptoss20-4096-mlock-fit+cache | 17.96 (17.03-19.4) | 4110 | MoE cache size =   958.12 MiB for 8471.78 MiB of host experts, 90% of the misses on th; hit rate = 11.49% |
| gptoss20-4096-mlock-fit | 15.53 (15.52-15.55) | 4074 |  |
| gptoss20-4096-mmap-cmoe+cache | 16.77 (14.89-18.74) | 3514 | MoE cache size =  1588.46 MiB for 9682.03 MiB of host experts, 90% of the misses on th; hit rate = 15.77% |
| gptoss20-4096-mmap-fit+cache | 17.36 (16.48-19.0) | 4092 | MoE cache size =   958.12 MiB for 8471.78 MiB of host experts, 95% of the misses on th; hit rate = 11.27% |
| gptoss20-4096-mmap-fit | 15.45 (15.44-15.46) | 4058 |  |
| gptoss20-8192-mlock-cmoe+cache | 52.89 (46.97-55.91) | 7646 | MoE cache size =  5685.67 MiB for 9682.03 MiB of host experts, 0% of the misses on the; hit rate = 55.27% |
| gptoss20-8192-mlock-fit+cache | 35.62 (34.25-37.39) | 8152 | MoE cache size =   958.12 MiB for 4437.60 MiB of host experts, 90% of the misses on th; hit rate = 19.67% |
| gptoss20-8192-mlock-fit | 27.95 (27.9-27.99) | 8110 |  |
| gptoss20-8192-mmap-cmoe+cache | 37.17 (34.2-42.46) | 7610 | MoE cache size =  5685.67 MiB for 9682.03 MiB of host experts, 0% of the misses on the; hit rate = 56.22% |
| gptoss20-8192-mmap-fit+cache | 33.77 (29.67-40.15) | 8128 | MoE cache size =   958.12 MiB for 4437.60 MiB of host experts, 95% of the misses on th; hit rate = 21.51% |
| gptoss20-8192-mmap-fit | 27.93 (27.9-27.95) | 8102 |  |
| qwen30b-4096-mlock-cmoe+cache | 34.78 (34.27-35.59) | 3552 | MoE cache size =  1471.29 MiB for 16740.00 MiB of host experts, 90% of the misses on t; hit rate = 12.55% |
| qwen30b-4096-mlock-fit+cache | 33.6 (32.98-34.2) | 4116 | MoE cache size =  1662.22 MiB for 16366.50 MiB of host experts, 90% of the misses on t; hit rate = 15.58% |
| qwen30b-4096-mlock-fit | 20.97 (20.9-21.01) | 4154 |  |
| qwen30b-4096-mmap-cmoe+cache | 26.77 (26.19-27.34) | 3520 | MoE cache size =  1471.29 MiB for 16740.00 MiB of host experts, 95% of the misses on t; hit rate = 12.75% |
| qwen30b-4096-mmap-fit+cache | 31.84 (31.51-32.32) | 4084 | MoE cache size =  1662.22 MiB for 16366.50 MiB of host experts, 95% of the misses on t; hit rate = 17.01% |
| qwen30b-4096-mmap-fit | 20.83 (20.79-20.89) | 4126 |  |
| qwen30b-8192-mlock-cmoe+cache | 65.45 (64.07-66.15) | 7644 | MoE cache size =  5563.65 MiB for 16740.00 MiB of host experts, 50% of the misses on t; hit rate = 42.02% |
| qwen30b-8192-mlock-fit+cache | 47.74 (44.1-51.0) | 8224 | MoE cache size =  1659.94 MiB for 12132.00 MiB of host experts, 90% of the misses on t; hit rate = 20.42% |
| qwen30b-8192-mlock-fit | 26.69 (26.63-26.76) | 8232 |  |
| qwen30b-8192-mmap-cmoe+cache | 57.38 (56.35-58.88) | 7616 | MoE cache size =  5563.65 MiB for 16740.00 MiB of host experts, 80% of the misses on t; hit rate = 43.87% |
| qwen30b-8192-mmap-fit+cache | 42.12 (39.21-44.31) | 8202 | MoE cache size =  1659.94 MiB for 12132.00 MiB of host experts, 95% of the misses on t; hit rate = 22.89% |
| qwen30b-8192-mmap-fit | 26.67 (26.66-26.69) | 8212 |  |
