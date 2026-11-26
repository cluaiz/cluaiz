| Phase | Seconds | Machine CPU avg / peak % | Machine memory peak MB | Cluaiz CPU avg / peak % | Cluaiz anon avg / peak MB | Cluaiz mapped file peak MB | Website down |
|---|---|---|---|---|---|---|---|
| A baseline; no Cluaiz | 29 | 4 / 42 | 619 | 0 / 0 | 0 / 0 | 0 | 0 |
| B pull chat models | 6 | 28 / 45 | 626 | 0 / 0 | 4 / 4 | 25 | 0 |
| B bench bonsai-4b-gguf | 209 | 100 / 100 | 1038 | 99 / 100 | 435 / 439 | 569 | 0 |
| B bench gemma-3-1b-it-gguf-Q4_K_M | 38 | 99 / 100 | 878 | 92 / 100 | 259 / 281 | 792 | 0 |
| B bench llama-3.2-1b-instruct-gguf-Q4_K_M | 31 | 99 / 100 | 1303 | 93 / 100 | 660 / 698 | 797 | 0 |
| B bench llama-3.2-3b-instruct-gguf-Q4_K_M | 92 | 99 / 100 | 2296 | 96 / 100 | 1582 / 1671 | 1952 | 0 |
| B bench phi-4-mini-instruct-gguf-Q4_K_M | 110 | 98 / 100 | 2260 | 96 / 100 | 1567 / 1651 | 2402 | 0 |
| B bench qwen3-0.6b-gguf-Q4_K_M | 18 | 98 / 100 | 1325 | 92 / 100 | 666 / 732 | 404 | 0 |
| B bench qwen3-1.7b-gguf-Q4_K_M | 53 | 98 / 100 | 1578 | 96 / 100 | 937 / 992 | 1082 | 0 |
| B bench qwen3-4b-gguf-Q4_K_M | 132 | 99 / 100 | 2727 | 96 / 100 | 2018 / 2124 | 2407 | 0 |
| B bench ternary-bonsai-2-4b-gguf | 250 | 100 / 100 | 1051 | 98 / 100 | 343 / 349 | 1173 | 0 |
| A server idle | 61 | 1 / 32 | 634 | 0 / 0 | 9 / 9 | 35 | 0 |
| C chat bonsai-4b-gguf | 32 | 98 / 100 | 956 | 97 / 100 | 362 / 374 | 582 | 0 |
| C chat gemma-3-1b-it-gguf-Q4_K_M | 12 | 96 / 100 | 839 | 93 / 100 | 220 / 252 | 806 | 0 |
| C chat llama-3.2-1b-instruct-gguf-Q4_K_M | 13 | 97 / 100 | 1244 | 93 / 100 | 577 / 661 | 813 | 0 |
| C chat llama-3.2-3b-instruct-gguf-Q4_K_M | 25 | 94 / 100 | 2232 | 91 / 100 | 1354 / 1630 | 1968 | 0 |
| C chat phi-4-mini-instruct-gguf-Q4_K_M | 33 | 94 / 100 | 2088 | 92 / 100 | 1242 / 1474 | 2415 | 0 |
| C chat qwen3-0.6b-gguf-Q4_K_M | 5 | 95 / 100 | 1334 | 92 / 100 | 542 / 741 | 414 | 0 |
| C chat qwen3-1.7b-gguf-Q4_K_M | 10 | 94 / 100 | 1576 | 92 / 100 | 818 / 971 | 1099 | 0 |
| C chat qwen3-4b-gguf-Q4_K_M | 36 | 92 / 100 | 2589 | 90 / 100 | 1556 / 1933 | 2424 | 0 |
| C chat ternary-bonsai-2-4b-gguf | 41 | 99 / 100 | 952 | 96 / 100 | 322 / 346 | 1189 | 0 |
| D speech | 28 | 99 / 100 | 2337 | 97 / 100 | 1074 / 1707 | 1207 | 0 |
| E embeddings | 12 | 98 / 100 | 2401 | 96 / 101 | 1778 / 1778 | 1434 | 0 |
| E ingest | 1 | 100 / 100 | 2388 | 90 / 90 | 1778 / 1778 | 1433 | 0 |
| F provider | 2 | 22 / 24 | 657 | 0 / 0 | 4 / 9 | 33 | 0 |
| G flow | 10 | 78 / 100 | 2219 | 72 / 99 | 786 / 1622 | 1986 | 0 |
| H agent provider | 4 | 100 / 100 | 2224 | 95 / 99 | 1624 / 1626 | 1337 | 0 |
| H agent local | 1 | 100 / 100 | 2224 | 86 / 86 | 1626 / 1626 | 1325 | 0 |
| I concurrency qwen3-0.6b-gguf-Q4_K_M | 15 | 98 / 100 | 2227 | 96 / 99 | 713 / 1621 | 1293 | 0 |
| I concurrency llama-3.2-3b-instruct-gguf-Q4_K_M | 41 | 97 / 100 | 2256 | 96 / 100 | 1530 / 1629 | 1971 | 0 |
| I mixed load | 184 | 100 / 100 | 2602 | 98 / 100 | 1446 / 1996 | 1102 | 0 |
| end | 22 | 3 / 54 | 661 | 0 / 0 | 0 / 0 | 0 | 0 |
