| round | config | pp512 | tg128 | GB read in tg phase | min free MiB in tg phase |
|---|---|---|---|---|---|
| 1 | C-debug | 160.54 | 10.58 | 0.60 | 156 |
| 1 | L | 85.9 | 14.49 | 0.83 | 109 |
| 1 | L-forkcpu | 156.87 | 15.58 | 0.61 | 526 |
| 2 | C-debug | 183.69 | 16.4 | 0.74 | 412 |
| 2 | L | 175.8 | 17.67 | 0.60 | 1325 |
| 2 | L-forkcpu | 149.58 | 13.56 | 1.10 | 723 |
| 3 | C-debug | 171.24 | 14.96 | 0.53 | 999 |
| 3 | L | 295.85 | 17.66 | 1.01 | 1287 |
| 3 | L-forkcpu | 299.43 | 17.14 | 0.03 | 1406 |
| 4 | C-debug | 193.49 | 18.6 | 0.25 | 1366 |
| 4 | L | 167.98 | 19.78 | 0.50 | 650 |
| 4 | L-forkcpu | 289.7 | 18.72 | 0.04 | 1105 |

Median over rounds >= 2 with at most 0.3 GB read in the tg phase:
| config | tg128 median | n | tg128 values | pp512 median |
|---|---|---|---|---|
| C-debug | 18.60 | 1 | 18.60 | 193.49 |
| L | ? | 0 |  | ? |
| L-forkcpu | 17.93 | 2 | 17.14, 18.72 | 294.56 |
