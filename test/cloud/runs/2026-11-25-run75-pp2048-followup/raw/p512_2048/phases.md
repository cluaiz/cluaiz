| run | t/s | phase | s | MiB read | proc major faults | KiB/fault | anon MiB | file MiB |
|---|---|---|---|---|---|---|---|---|
| r1_C | pp512 51.90 pp2048 93.38 | start | 111 | 19901 | 2519 | 8090 | 230 | 8270 |
| r1_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 194 | 12997 | 452117 | 29 | 413 | 11733 |
| r1_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 69 | 6205 | 32066 | 198 | 432 | 11714 |
| r1_LF |  | benchmark 1/2: starting | 101 | 18241 | 121 | 154369 | 216 | 8056 |
| r1_LF |  | benchmark 1/2: warmup prompt run | 368 | 10122 | 1159147 | 9 | 388 | 11760 |
| r1_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1490 | 4751 | 321 | 415 | 11732 |
| r1_LF |  | benchmark 1/2: prompt run 2/2 | 8 | 1014 | 5812 | 179 | 424 | 11723 |
| r1_LF |  | benchmark 2/2: warmup prompt run | 22 | 2058 | 8045 | 262 | 440 | 11708 |
| r1_LF |  | benchmark 2/2: prompt run 1/2 | 19 | 1621 | 5807 | 286 | 442 | 11704 |
| r1_LF |  | benchmark 2/2: prompt run 2/2 | 19 | 1580 | 6313 | 256 | 444 | 11702 |
| r2_C | pp512 51.53 pp2048 92.11 | start | 111 | 19898 | 2517 | 8095 | 229 | 8270 |
| r2_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 196 | 13069 | 451927 | 30 | 413 | 11733 |
| r2_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 70 | 6329 | 33105 | 196 | 432 | 11714 |
| r2_LF |  | benchmark 1/2: starting | 101 | 18248 | 121 | 154426 | 216 | 8060 |
| r2_LF |  | benchmark 1/2: warmup prompt run | 369 | 10067 | 1159132 | 9 | 379 | 11768 |
| r2_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1493 | 4831 | 316 | 415 | 11732 |
| r2_LF |  | benchmark 1/2: prompt run 2/2 | 8 | 1004 | 5913 | 174 | 423 | 11724 |
| r2_LF |  | benchmark 2/2: warmup prompt run | 23 | 2095 | 7799 | 275 | 440 | 11707 |
| r2_LF |  | benchmark 2/2: prompt run 1/2 | 17 | 1490 | 3740 | 408 | 443 | 11704 |
| r2_LF |  | benchmark 2/2: prompt run 2/2 | 19 | 1499 | 6026 | 255 | 423 | 11703 |
| r3_C | pp512 51.31 pp2048 97.52 | start | 111 | 19896 | 2518 | 8091 | 227 | 8269 |
| r3_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 196 | 13078 | 449937 | 30 | 424 | 11722 |
| r3_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 67 | 6165 | 28950 | 218 | 448 | 11698 |
| r3_LF |  | benchmark 1/2: starting | 102 | 18416 | 129 | 146188 | 219 | 8064 |
| r3_LF |  | benchmark 1/2: warmup prompt run | 371 | 9901 | 1159629 | 9 | 382 | 11766 |
| r3_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1494 | 4705 | 325 | 407 | 11740 |
| r3_LF |  | benchmark 1/2: prompt run 2/2 | 8 | 1017 | 5768 | 181 | 416 | 11731 |
| r3_LF |  | benchmark 2/2: warmup prompt run | 22 | 2072 | 8226 | 258 | 437 | 11710 |
| r3_LF |  | benchmark 2/2: prompt run 1/2 | 21 | 1743 | 7099 | 251 | 440 | 11707 |
| r3_LF |  | benchmark 2/2: prompt run 2/2 | 19 | 1563 | 4746 | 337 | 441 | 11705 |
