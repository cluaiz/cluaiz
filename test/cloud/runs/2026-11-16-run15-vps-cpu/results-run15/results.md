### bonsai-4b-gguf_cpu_cluaiz
| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen3 4B Q1_0 | 0.53 GiB | 4.02 B | CPU only | 0 | 4096 | pp512 | 7.82 ± 0.09 |
| qwen3 4B Q1_0 | 0.53 GiB | 4.02 B | CPU only | 0 | 4096 | tg128 | 6.49 ± 0.05 |
### bonsai-4b-gguf_cpu_llama
| model                          |       size |     params | backend    | threads |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | --------------: | -------------------: |
| qwen3 4B Q1_0                  | 540.09 MiB |     4.02 B | CPU        |       1 |           pp512 |          7.77 ± 0.03 |
| qwen3 4B Q1_0                  | 540.09 MiB |     4.02 B | CPU        |       1 |           tg128 |          6.56 ± 0.09 |
### llama-3.2-3b-instruct-gguf-Q4_K_M_cpu_cluaiz
| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | CPU only | 0 | 4096 | pp512 | 21.17 ± 0.23 |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | CPU only | 0 | 4096 | tg128 | 8.16 ± 0.20 |
### llama-3.2-3b-instruct-gguf-Q4_K_M_cpu_llama
| model                          |       size |     params | backend    | threads |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | --------------: | -------------------: |
| llama 3B Q4_K - Medium         |   1.87 GiB |     3.21 B | CPU        |       1 |           pp512 |         21.80 ± 0.66 |
| llama 3B Q4_K - Medium         |   1.87 GiB |     3.21 B | CPU        |       1 |           tg128 |          7.60 ± 0.84 |
### ternary-bonsai-2-4b-gguf_cpu_cluaiz
### ternary-bonsai-2-4b-gguf_cpu_llama
| model                          |       size |     params | backend    | threads |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | --------------: | -------------------: |
### bonsai-4b-gguf_auto_cluaiz
| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| qwen3 4B Q1_0 | 0.53 GiB | 4.02 B | CPU only | 0 | 4096 | pp512 | 6.75 ± 0.16 |
| qwen3 4B Q1_0 | 0.53 GiB | 4.02 B | CPU only | 0 | 4096 | tg128 | 5.79 ± 0.09 |
### bonsai-4b-gguf_auto_llama
| model                          |       size |     params | backend    | threads | type_k | type_v |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | -----: | -----: | --------------: | -------------------: |
| qwen3 4B Q1_0                  | 540.09 MiB |     4.02 B | CPU        |       1 |   q8_0 |   q8_0 |           pp512 |          6.71 ± 0.14 |
| qwen3 4B Q1_0                  | 540.09 MiB |     4.02 B | CPU        |       1 |   q8_0 |   q8_0 |           tg128 |          5.99 ± 0.21 |
### llama-3.2-3b-instruct-gguf-Q4_K_M_auto_cluaiz
| model | size | params | placement | ngl | n_ctx | test | t/s |
| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | CPU only | 0 | 4096 | pp512 | 17.10 ± 0.27 |
| llama 3B Q4_K - Medium | 1.87 GiB | 3.21 B | CPU only | 0 | 4096 | tg128 | 7.86 ± 0.65 |
### llama-3.2-3b-instruct-gguf-Q4_K_M_auto_llama
| model                          |       size |     params | backend    | threads | type_k | type_v |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | -----: | -----: | --------------: | -------------------: |
| llama 3B Q4_K - Medium         |   1.87 GiB |     3.21 B | CPU        |       1 |   q8_0 |   q8_0 |           pp512 |         16.71 ± 0.16 |
| llama 3B Q4_K - Medium         |   1.87 GiB |     3.21 B | CPU        |       1 |   q8_0 |   q8_0 |           tg128 |          7.73 ± 0.06 |
### ternary-bonsai-2-4b-gguf_auto_cluaiz
### ternary-bonsai-2-4b-gguf_auto_llama
| model                          |       size |     params | backend    | threads |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | --------------: | -------------------: |
