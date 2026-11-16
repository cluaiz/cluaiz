### olmoe-1b-7b-0125-instruct-gguf-Q4_K_M_auto_cluaiz
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | pp512 | 1072.03 ± 315.13 |
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | tg128 | 77.39 ± 4.51 |
### olmoe-1b-7b-0125-instruct-gguf-Q4_K_M_auto_llama
| olmoe A1.7B Q4_K - Medium      |   3.92 GiB |     6.92 B | CUDA       |  99 |         16 |           pp512 |      1071.12 ± 34.93 |
| olmoe A1.7B Q4_K - Medium      |   3.92 GiB |     6.92 B | CUDA       |  99 |         16 |           tg128 |         46.67 ± 3.38 |
### olmoe-1b-7b-0125-instruct-gguf-Q4_K_M_autonocache_cluaiz
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | pp512 | 1182.97 ± 333.97 |
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | MoE split (attention on GPU, experts in RAM) | -1 | 4096 | tg128 | 65.31 ± 1.07 |
### olmoe-1b-7b-0125-instruct-gguf-Q4_K_M_cpu_cluaiz
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | CPU only | 0 | 4096 | pp512 | 96.83 ± 3.30 |
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | CPU only | 0 | 4096 | tg128 | 42.73 ± 3.18 |
### olmoe-1b-7b-0125-instruct-gguf-Q4_K_M_cpu_llama
| olmoe A1.7B Q4_K - Medium      |   3.92 GiB |     6.92 B | CUDA       |   0 |           pp512 |      867.52 ± 138.41 |
| olmoe A1.7B Q4_K - Medium      |   3.92 GiB |     6.92 B | CUDA       |   0 |           tg128 |         46.33 ± 0.21 |
### olmoe-1b-7b-0125-instruct-gguf-Q4_K_M_split_cluaiz
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | GPU + RAM split | 9 | 4096 | pp512 | 1266.36 ± 307.69 |
| olmoe A1.7B Q4_K - Medium | 3.92 GiB | 6.92 B | GPU + RAM split | 9 | 4096 | tg128 | 67.88 ± 0.21 |
### olmoe-1b-7b-0125-instruct-gguf-Q4_K_M_split_llama
| olmoe A1.7B Q4_K - Medium      |   3.92 GiB |     6.92 B | CUDA       |   9 |           pp512 |     1383.97 ± 182.31 |
| olmoe A1.7B Q4_K - Medium      |   3.92 GiB |     6.92 B | CUDA       |   9 |           tg128 |         67.25 ± 0.47 |
### qwen3.5-4b-gguf-Q4_K_M_auto_cluaiz
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | GPU + RAM split | 15 | 8192 | pp512 | 1139.26 ± 58.01 |
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | GPU + RAM split | 15 | 8192 | tg128 | 10.89 ± 3.34 |
### qwen3.5-4b-gguf-Q4_K_M_auto_llama
| qwen35 4B Q4_K - Medium        |   2.54 GiB |     4.21 B | CUDA       |  15 |   q8_0 |   q8_0 |           pp512 |      1146.88 ± 91.69 |
| qwen35 4B Q4_K - Medium        |   2.54 GiB |     4.21 B | CUDA       |  15 |   q8_0 |   q8_0 |           tg128 |         10.13 ± 2.86 |
### qwen3.5-4b-gguf-Q4_K_M_cpu_cluaiz
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | CPU only | 0 | 4096 | pp512 | 51.58 ± 0.58 |
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | CPU only | 0 | 4096 | tg128 | 11.16 ± 0.24 |
### qwen3.5-4b-gguf-Q4_K_M_cpu_llama
| qwen35 4B Q4_K - Medium        |   2.54 GiB |     4.21 B | CUDA       |   0 |           pp512 |       836.34 ± 54.94 |
| qwen35 4B Q4_K - Medium        |   2.54 GiB |     4.21 B | CUDA       |   0 |           tg128 |          8.50 ± 0.14 |
### qwen3.5-4b-gguf-Q4_K_M_split_cluaiz
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | GPU + RAM split | 17 | 4096 | pp512 | 1224.93 ± 74.14 |
| qwen35 4B Q4_K - Medium | 2.54 GiB | 4.21 B | GPU + RAM split | 17 | 4096 | tg128 | 12.88 ± 3.92 |
### qwen3.5-4b-gguf-Q4_K_M_split_llama
| qwen35 4B Q4_K - Medium        |   2.54 GiB |     4.21 B | CUDA       |  17 |           pp512 |      1221.63 ± 84.25 |
| qwen35 4B Q4_K - Medium        |   2.54 GiB |     4.21 B | CUDA       |  17 |           tg128 |         12.54 ± 3.91 |
### ternary-bonsai-2-4b-gguf_auto_cluaiz
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | Full GPU | -1 | 16384 | pp512 | 1954.40 ± 95.66 |
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | Full GPU | -1 | 16384 | tg128 | 68.90 ± 1.42 |
### ternary-bonsai-2-4b-gguf_auto_llama
| qwen35 4B Q2_0                 |   1.12 GiB |     4.21 B | CUDA       |  99 |           pp512 |      2028.37 ± 30.21 |
| qwen35 4B Q2_0                 |   1.12 GiB |     4.21 B | CUDA       |  99 |           tg128 |         70.80 ± 1.28 |
### ternary-bonsai-2-4b-gguf_cpu_cluaiz
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | CPU only | 0 | 4096 | pp512 | 22.74 ± 0.12 |
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | CPU only | 0 | 4096 | tg128 | 14.22 ± 0.28 |
### ternary-bonsai-2-4b-gguf_cpu_llama
| qwen35 4B Q2_0                 |   1.12 GiB |     4.21 B | CUDA       |   0 |           pp512 |      1051.94 ± 93.85 |
| qwen35 4B Q2_0                 |   1.12 GiB |     4.21 B | CUDA       |   0 |           tg128 |         10.13 ± 1.06 |
### ternary-bonsai-2-4b-gguf_gpu_cluaiz
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | Full GPU | -1 | 4096 | pp512 | 1925.29 ± 113.74 |
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | Full GPU | -1 | 4096 | tg128 | 70.09 ± 1.45 |
### ternary-bonsai-2-4b-gguf_gpu_llama
| qwen35 4B Q2_0                 |   1.12 GiB |     4.21 B | CUDA       |  99 |           pp512 |      2011.17 ± 31.42 |
| qwen35 4B Q2_0                 |   1.12 GiB |     4.21 B | CUDA       |  99 |           tg128 |         69.80 ± 0.73 |
### ternary-bonsai-2-4b-gguf_split_cluaiz
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | GPU + RAM split | 17 | 4096 | pp512 | 1310.21 ± 87.60 |
| qwen35 4B Q2_0 | 1.12 GiB | 4.21 B | GPU + RAM split | 17 | 4096 | tg128 | 17.72 ± 4.61 |
### ternary-bonsai-2-4b-gguf_split_llama
| qwen35 4B Q2_0                 |   1.12 GiB |     4.21 B | CUDA       |  17 |           pp512 |      1298.21 ± 80.80 |
| qwen35 4B Q2_0                 |   1.12 GiB |     4.21 B | CUDA       |  17 |           tg128 |         15.65 ± 4.31 |
