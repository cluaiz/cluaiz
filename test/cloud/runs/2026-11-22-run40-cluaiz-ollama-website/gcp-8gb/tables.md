## gcp-8gb

Machine: Intel(R) Xeon(R) CPU @ 2.20GHz, 2 vCPU, 7934 MB RAM; threads 2; Cluaiz commit b8ef5f64a0a3ec53281d39b80fa52d0879ae9891

### W1 chat (ticket label as JSON)

| Round | Engine | Model | At once | OK | Seconds per request p50 | First token p50 | Gen t/s p50 | Out tokens p50 | Right label | Engine memory peak MB | Website during step |
|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|
| r1 | cluaiz | qwen3-0.6b | 1 | 64/64 | 1.349 | 0.726 | 16.14 | 10.0 | 0.2812 | 534 | 2685/2685 ok, p95 12 ms |
| r1 | cluaiz | qwen3-0.6b | 4 | 71/71 | 5.385 | 4.762 | 15.92 | 10 | 0.2817 | 534 | 2937/2937 ok, p95 11 ms |
| r1 | cluaiz | qwen3-1.7b | 1 | 30/30 | 2.806 | 1.945 | 6.96 | 6.0 | 0.8333 | 1004 | 2706/2706 ok, p95 12 ms |
| r1 | cluaiz | qwen3-1.7b | 4 | 35/35 | 11.394 | 10.52 | 6.89 | 6 | 0.8571 | 1004 | 2976/2976 ok, p95 11 ms |
| r1 | ollama | qwen3-0.6b | 1 | 70/70 | 1.315 | 0.744 | 23.6 | 15.0 | 0.2857 | 788 | 2727/2727 ok, p95 12 ms |
| r1 | ollama | qwen3-0.6b | 4 | 83/83 | 4.477 | 3.814 | 23.32 | 15 | 0.2651 | 788 | 2769/2769 ok, p95 12 ms |
| r1 | ollama | qwen3-1.7b | 1 | 26/26 | 3.712 | 2.21 | 9.38 | 14.0 | 0.8462 | 1448 | 2727/2727 ok, p95 12 ms |
| r1 | ollama | qwen3-1.7b | 4 | 40/40 | 9.925 | 8.372 | 9.52 | 14.0 | 0.875 | 1448 | 2898/2898 ok, p95 12 ms |
| r2 | cluaiz | qwen3-0.6b | 1 | 67/67 | 1.316 | 0.711 | 16.44 | 10 | 0.2836 | 560 | 2730/2730 ok, p95 10 ms |
| r2 | cluaiz | qwen3-0.6b | 4 | 72/72 | 5.273 | 4.676 | 16.62 | 10.0 | 0.2778 | 560 | 2829/2829 ok, p95 10 ms |
| r2 | cluaiz | qwen3-1.7b | 1 | 31/31 | 2.772 | 1.922 | 7.07 | 6 | 0.8387 | 1004 | 2769/2769 ok, p95 11 ms |
| r2 | cluaiz | qwen3-1.7b | 4 | 35/35 | 11.279 | 10.405 | 6.99 | 6 | 0.8571 | 1004 | 2933/2933 ok, p95 11 ms |
| r2 | ollama | qwen3-0.6b | 1 | 71/71 | 1.314 | 0.769 | 23.18 | 15 | 0.2817 | 788 | 2709/2709 ok, p95 11 ms |
| r2 | ollama | qwen3-0.6b | 4 | 83/83 | 4.52 | 3.861 | 23.12 | 15 | 0.2651 | 788 | 2865/2865 ok, p95 11 ms |
| r2 | ollama | qwen3-1.7b | 1 | 26/26 | 3.726 | 2.172 | 9.46 | 14.0 | 0.8462 | 1448 | 2682/2682 ok, p95 11 ms |
| r2 | ollama | qwen3-1.7b | 4 | 40/40 | 10.152 | 8.57 | 9.46 | 14.0 | 0.875 | 1448 | 2989/2989 ok, p95 12 ms |

### W4 embeddings and W5 speech (Cluaiz)

| Workload | Part | At once | OK | HTTP 429 | p50 s | p95 s |
|---|---|---:|---|---:|---:|---:|
| W4 | index_and_search | 1 | None/None | 0 | None | None |
| W4 | ramp | 1 | 10/10 | 0 | 9.269 | 9.47 |
| W4 | ramp | 2 | 12/12 | 0 | 17.78 | 18.354 |
| W4 | ramp | 4 | 14/14 | 0 | 35.498 | 36.446 |
| W5 | ramp | 1 | 17/17 | 0 | 5.097 | 13.026 |
| W5 | ramp | 2 | 18/18 | 0 | 10.535 | 15.518 |
| W5 | ramp | 4 | 21/21 | 0 | 17.139 | 31.122 |

Website, whole run: 88212/88212 requests OK, p50 4 ms, p95 11 ms, max 163 ms

### CPU steal between blocks

- r1_cluaiz_W1: steal 0.0%, busy 90.4%
- r1_llama_W1: steal 0.0%, busy 17.1%
- r1_ollama_W1: steal 0.0%, busy 92.6%
- r2_cluaiz_W1: steal 0.0%, busy 90.4%
- r2_llama_W1: steal 0.0%, busy 17.2%
- r2_ollama_W1: steal 0.0%, busy 92.8%
- cluaiz_W4_W5: steal 0.0%, busy 80.5%
