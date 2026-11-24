| run | t/s | phase | s | MiB read | proc major faults | KiB/fault | anon MiB | file MiB |
|---|---|---|---|---|---|---|---|---|
| r1_C | pp512 49.93 pp2048 88.81 | start | 112 | 19999 | 2613 | 7837 | 228 | 8290 |
| r1_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 184 | 13201 | 451755 | 30 | 424 | 11720 |
| r1_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 71 | 6959 | 40530 | 176 | 447 | 11698 |
| r1_LF |  | benchmark 1/2: starting | 101 | 18416 | 128 | 147329 | 219 | 8098 |
| r1_LF |  | benchmark 1/2: warmup prompt run | 351 | 9909 | 1159730 | 9 | 382 | 11762 |
| r1_LF |  | benchmark 1/2: prompt run 1/2 | 8 | 1350 | 4974 | 278 | 408 | 11738 |
| r1_LF |  | benchmark 1/2: prompt run 2/2 | 9 | 1174 | 7759 | 155 | 416 | 11730 |
| r1_LF |  | benchmark 2/2: warmup prompt run | 22 | 2128 | 7626 | 286 | 435 | 11712 |
| r1_LF |  | benchmark 2/2: prompt run 1/2 | 18 | 1637 | 5441 | 308 | 438 | 11709 |
| r1_LF |  | benchmark 2/2: prompt run 2/2 | 18 | 1461 | 3899 | 384 | 441 | 11704 |
| r2_C | pp512 51.22 pp2048 100.89 | start | 112 | 20006 | 2653 | 7722 | 230 | 8298 |
| r2_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 186 | 13235 | 453404 | 30 | 414 | 11731 |
| r2_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 64 | 6066 | 26535 | 234 | 433 | 11712 |
| r2_LF |  | benchmark 1/2: starting | 100 | 18237 | 120 | 155626 | 219 | 8063 |
| r2_LF |  | benchmark 1/2: warmup prompt run | 351 | 10007 | 1159937 | 9 | 383 | 11761 |
| r2_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1532 | 5451 | 288 | 409 | 11738 |
| r2_LF |  | benchmark 1/2: prompt run 2/2 | 8 | 1048 | 7028 | 153 | 418 | 11729 |
| r2_LF |  | benchmark 2/2: warmup prompt run | 23 | 2317 | 9940 | 239 | 436 | 11710 |
| r2_LF |  | benchmark 2/2: prompt run 1/2 | 19 | 1910 | 9229 | 212 | 440 | 11707 |
| r2_LF |  | benchmark 2/2: prompt run 2/2 | 22 | 2088 | 10842 | 197 | 442 | 11703 |
| r3_C | pp512 50.73 pp2048 97.94 | start | 112 | 20003 | 2633 | 7779 | 228 | 8298 |
| r3_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 185 | 13102 | 451566 | 30 | 421 | 11723 |
| r3_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 66 | 6348 | 28863 | 225 | 446 | 11699 |
| r3_LF |  | benchmark 1/2: starting | 101 | 18412 | 128 | 147293 | 219 | 8099 |
| r3_LF |  | benchmark 1/2: warmup prompt run | 352 | 9810 | 1159429 | 9 | 382 | 11763 |
| r3_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1518 | 5457 | 285 | 408 | 11738 |
| r3_LF |  | benchmark 1/2: prompt run 2/2 | 6 | 698 | 3860 | 185 | 417 | 11729 |
| r3_LF |  | benchmark 2/2: warmup prompt run | 26 | 2579 | 15622 | 169 | 437 | 11709 |
| r3_LF |  | benchmark 2/2: prompt run 1/2 | 19 | 1752 | 7401 | 242 | 441 | 11705 |
| r3_LF |  | benchmark 2/2: prompt run 2/2 | 19 | 1651 | 6118 | 276 | 442 | 11703 |
| r4_C | pp512 48.81 pp2048 88.17 | start | 112 | 20003 | 2664 | 7689 | 231 | 8300 |
| r4_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 188 | 13326 | 454196 | 30 | 415 | 11730 |
| r4_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 72 | 6972 | 42134 | 169 | 434 | 11711 |
| r4_LF |  | benchmark 1/2: starting | 100 | 18242 | 120 | 155662 | 219 | 8066 |
| r4_LF |  | benchmark 1/2: warmup prompt run | 352 | 9968 | 1159511 | 9 | 384 | 11763 |
| r4_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1509 | 5370 | 288 | 409 | 11736 |
| r4_LF |  | benchmark 1/2: prompt run 2/2 | 9 | 1179 | 7326 | 165 | 418 | 11729 |
| r4_LF |  | benchmark 2/2: warmup prompt run | 23 | 2175 | 9179 | 243 | 436 | 11711 |
| r4_LF |  | benchmark 2/2: prompt run 1/2 | 17 | 1507 | 3802 | 406 | 441 | 11705 |
| r4_LF |  | benchmark 2/2: prompt run 2/2 | 18 | 1438 | 4254 | 346 | 443 | 11701 |
| r5_C | pp512 49.44 pp2048 90.18 | start | 112 | 20004 | 2654 | 7718 | 229 | 8301 |
| r5_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 188 | 13220 | 452575 | 30 | 424 | 11721 |
| r5_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 71 | 6800 | 38530 | 181 | 439 | 11700 |
| r5_LF |  | benchmark 1/2: starting | 101 | 18413 | 128 | 147303 | 219 | 8066 |
| r5_LF |  | benchmark 1/2: warmup prompt run | 351 | 9823 | 1159451 | 9 | 382 | 11759 |
| r5_LF |  | benchmark 1/2: prompt run 1/2 | 8 | 1338 | 4847 | 283 | 408 | 11738 |
| r5_LF |  | benchmark 1/2: prompt run 2/2 | 6 | 723 | 3158 | 234 | 418 | 11728 |
| r5_LF |  | benchmark 2/2: warmup prompt run | 25 | 2389 | 15121 | 162 | 439 | 11708 |
| r5_LF |  | benchmark 2/2: prompt run 1/2 | 19 | 1717 | 7282 | 241 | 443 | 11703 |
| r5_LF |  | benchmark 2/2: prompt run 2/2 | 18 | 1633 | 5728 | 292 | 446 | 11699 |
