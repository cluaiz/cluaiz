| run | t/s | phase | s | MiB read | proc major faults | KiB/fault | anon MiB | file MiB |
|---|---|---|---|---|---|---|---|---|
| r1_C | pp2048 88.13 | start | 113 | 20242 | 2608 | 7948 | 227 | 8281 |
| r1_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 248 | 18006 | 476213 | 39 | 415 | 11731 |
| r1_LF |  | benchmark 1/1: starting | 101 | 18250 | 121 | 154442 | 219 | 8059 |
| r1_LF |  | benchmark 1/1: warmup prompt run | 398 | 13567 | 1173956 | 12 | 382 | 11765 |
| r1_LF |  | benchmark 1/1: prompt run 1/2 | 21 | 1821 | 8528 | 219 | 422 | 11725 |
| r1_LF |  | benchmark 1/1: prompt run 2/2 | 21 | 1728 | 7446 | 238 | 433 | 11715 |
| r2_C | pp2048 93.60 | start | 111 | 19896 | 2514 | 8104 | 227 | 8269 |
| r2_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 245 | 17783 | 474796 | 38 | 409 | 11735 |
| r2_LF |  | benchmark 1/1: starting | 101 | 18259 | 121 | 154520 | 219 | 8061 |
| r2_LF |  | benchmark 1/1: warmup prompt run | 396 | 13533 | 1173421 | 12 | 383 | 11764 |
| r2_LF |  | benchmark 1/1: prompt run 1/2 | 22 | 1913 | 9192 | 213 | 422 | 11725 |
| r2_LF |  | benchmark 1/1: prompt run 2/2 | 21 | 1803 | 9473 | 195 | 431 | 11717 |
| r3_C | pp2048 93.50 | start | 111 | 19894 | 2512 | 8110 | 227 | 8266 |
| r3_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 247 | 17802 | 472978 | 39 | 411 | 11735 |
| r3_LF |  | benchmark 1/1: starting | 101 | 18250 | 121 | 154445 | 219 | 8060 |
| r3_LF |  | benchmark 1/1: warmup prompt run | 399 | 13625 | 1173635 | 12 | 384 | 11764 |
| r3_LF |  | benchmark 1/1: prompt run 1/2 | 21 | 1830 | 8796 | 213 | 421 | 11726 |
| r3_LF |  | benchmark 1/1: prompt run 2/2 | 21 | 1797 | 8735 | 211 | 432 | 11714 |
