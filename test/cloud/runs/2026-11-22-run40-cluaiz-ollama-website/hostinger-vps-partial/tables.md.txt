## hostinger-vps-partial

Machine: AMD EPYC 9354P 32-Core Processor, 1 vCPU, 3915 MB RAM; threads 1; Cluaiz commit b8ef5f64a0a3ec53281d39b80fa52d0879ae9891

### W1 chat (ticket label as JSON)

| Round | Engine | Model | At once | OK | Seconds per request p50 | First token p50 | Gen t/s p50 | Out tokens p50 | Right label | Engine memory peak MB | Website during step |
|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| r1 | cluaiz | qwen3-0.6b | 1 | 186/186 | 0.625 | 0.343 | 35.04 | 10.0 | 0.2849 | 559 | - |
| r1 | cluaiz | qwen3-0.6b | 4 | 190/190 | 2.532 | 2.252 | 34.52 | 10.0 | 0.2789 | 559 | - |
| r1 | cluaiz | qwen3-1.7b | 1 | 81/81 | 1.383 | 0.942 | 14.66 | 6 | 0.8519 | 1002 | - |
| r1 | cluaiz | qwen3-1.7b | 4 | 93/93 | 5.341 | 4.88 | 15.12 | 6 | 0.828 | 1002 | - |
| r1 | llama | qwen3-0.6b | 1 | 194/194 | 0.629 | 0.315 | 46.36 | 15.0 | 0.2784 | 678 | - |
| r1 | llama | qwen3-0.6b | 4 | 207/207 | 2.327 | 1.988 | 45.91 | 15 | 0.285 | 678 | - |
| r1 | llama | qwen3-1.7b | 1 | 31/31 | 4.32 | 2.717 | 8.25 | 14 | 0.8387 | 1145 | - |
| r1 | llama | qwen3-1.7b | 4 | 45/45 | 11.591 | 9.794 | 8.26 | 14 | 0.8444 | 1145 | - |

### CPU steal between blocks

- r1_cluaiz_W1: steal 0.6%, busy 99.6%
