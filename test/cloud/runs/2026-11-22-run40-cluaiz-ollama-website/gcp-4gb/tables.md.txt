## gcp-4gb

Machine: Intel(R) Xeon(R) CPU @ 2.20GHz, 2 vCPU, 3908 MB RAM; threads 2; Cluaiz commit b8ef5f64a0a3ec53281d39b80fa52d0879ae9891

### W1 chat (ticket label as JSON)

| Round | Engine | Model | At once | OK | Seconds per request p50 | First token p50 | Gen t/s p50 | Out tokens p50 | Right label | Engine memory peak MB | Website during step |
|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| r1 | cluaiz | qwen3-0.6b | 1 | 55/55 | 1.582 | 0.781 | 12.57 | 10 | 0.3091 | 560 | 2660/2660 ok, p95 19 ms |
| r1 | cluaiz | qwen3-0.6b | 4 | 60/60 | 6.244 | 5.471 | 12.63 | 10.0 | 0.3 | 560 | 2845/2845 ok, p95 16 ms |
| r1 | cluaiz | qwen3-1.7b | 1 | 26/26 | 3.278 | 2.11 | 5.25 | 6.0 | 0.8462 | 1004 | 2719/2719 ok, p95 17 ms |
| r1 | cluaiz | qwen3-1.7b | 4 | 31/31 | 13.249 | 11.959 | 5.32 | 6 | 0.8387 | 1004 | 3057/3057 ok, p95 18 ms |
| r1 | ollama | qwen3-0.6b | 1 | 56/56 | 1.635 | 0.867 | 18.05 | 15.0 | 0.3036 | 788 | 2787/2787 ok, p95 15 ms |
| r1 | ollama | qwen3-0.6b | 4 | 69/69 | 5.434 | 4.643 | 17.94 | 15 | 0.2899 | 788 | 2814/2814 ok, p95 16 ms |
| r1 | ollama | qwen3-1.7b | 1 | 19/19 | 4.515 | 2.539 | 7.09 | 14 | 0.7895 | 1440 | 2719/2719 ok, p95 16 ms |
| r1 | ollama | qwen3-1.7b | 4 | 33/33 | 12.443 | 10.421 | 7.27 | 14 | 0.8485 | 1440 | 2952/2952 ok, p95 16 ms |
| r2 | cluaiz | qwen3-0.6b | 1 | 57/57 | 1.542 | 0.787 | 13.06 | 10 | 0.2982 | 560 | 2756/2756 ok, p95 15 ms |
| r2 | cluaiz | qwen3-0.6b | 4 | 62/62 | 6.122 | 5.363 | 13.07 | 10.0 | 0.2903 | 560 | 2818/2818 ok, p95 15 ms |
| r2 | cluaiz | qwen3-1.7b | 1 | 26/26 | 3.218 | 2.101 | 5.45 | 6.0 | 0.8462 | 1004 | 2646/2646 ok, p95 15 ms |
| r2 | cluaiz | qwen3-1.7b | 4 | 31/31 | 13.166 | 11.92 | 5.42 | 6 | 0.8387 | 1004 | 3045/3045 ok, p95 16 ms |
| r2 | ollama | qwen3-0.6b | 1 | 56/56 | 1.587 | 0.887 | 17.98 | 15.0 | 0.3036 | 782 | 2724/2724 ok, p95 16 ms |
| r2 | ollama | qwen3-0.6b | 4 | 64/64 | 5.823 | 4.993 | 16.72 | 15.0 | 0.2812 | 782 | 2841/2841 ok, p95 19 ms |
| r2 | ollama | qwen3-1.7b | 1 | 14/14 | 5.854 | 3.337 | 5.56 | 14.0 | 0.8571 | 1444 | 2793/2793 ok, p95 30 ms |
| r2 | ollama | qwen3-1.7b | 4 | 29/29 | 13.417 | 11.881 | 6.41 | 14 | 0.8276 | 1444 | 3105/3105 ok, p95 19 ms |

### W4 embeddings and W5 speech (Cluaiz)

| Workload | Part | At once | OK | HTTP 429 | p50 s | p95 s |
|---|---|---:|---|---:|---:|---:|
| W4 | index_and_search | 1 | None/None | 0 | None | None |
| W4 | ramp | 1 | 9/9 | 0 | 10.247 | 11.385 |
| W4 | ramp | 2 | 11/11 | 0 | 19.51 | 20.457 |
| W4 | ramp | 4 | 13/13 | 0 | 38.386 | 38.773 |
| W5 | ramp | 1 | 15/15 | 0 | 6.033 | 15.422 |
| W5 | ramp | 2 | 16/16 | 0 | 12.234 | 18.81 |
| W5 | ramp | 4 | 19/19 | 0 | 21.357 | 37.23 |

Website, whole run: 90111/90111 requests OK, p50 7 ms, p95 16 ms, max 407 ms

### CPU steal between blocks

- r1_cluaiz_W1: steal 0.2%, busy 90.8%
- r1_llama_W1: steal 0.1%, busy 20.7%
- r1_ollama_W1: steal 0.1%, busy 92.1%
- r2_cluaiz_W1: steal 0.1%, busy 90.3%
- r2_llama_W1: steal 0.0%, busy 20.2%
- r2_ollama_W1: steal 2.1%, busy 92.0%
- cluaiz_W4_W5: steal 0.1%, busy 82.2%
