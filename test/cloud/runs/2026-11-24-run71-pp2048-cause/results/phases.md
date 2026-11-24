| run | t/s | phase | s | MiB read | proc major faults | KiB/fault | anon MiB | file MiB |
|---|---|---|---|---|---|---|---|---|
| r1_C-b2048 | pp512 50.53 pp2048 107.26 | start | 112 | 20011 | 2626 | 7803 | 229 | 8306 |
| r1_C-b2048 |  | running pp512 on MoE split (attention on GPU, experts in RAM | 186 | 13149 | 444157 | 30 | 417 | 11728 |
| r1_C-b2048 |  | running pp2048 on MoE split (attention on GPU, experts in RA | 63 | 5806 | 23475 | 253 | 439 | 11706 |
| r1_C-c2048 | pp512 50.42 pp2048 100.36 | start | 112 | 20095 | 2600 | 7914 | 228 | 8275 |
| r1_C-c2048 |  | running pp512 on MoE split (attention on GPU, experts in RAM | 190 | 13172 | 459654 | 29 | 411 | 11734 |
| r1_C-c2048 |  | running pp2048 on MoE split (attention on GPU, experts in RA | 66 | 6052 | 27703 | 224 | 425 | 11714 |
| r1_C-nw | pp512 50.89 pp2048 90.48 | start | 103 | 18502 | 1009 | 18777 | 218 | 7999 |
| r1_C-nw |  | running pp512 on MoE split (attention on GPU, experts in RAM | 376 | 13185 | 1157498 | 12 | 391 | 11753 |
| r1_C-nw |  | running pp2048 on MoE split (attention on GPU, experts in RA | 69 | 6622 | 33344 | 203 | 434 | 11711 |
| r1_C | pp512 49.86 pp2048 101.48 | start | 112 | 20091 | 2595 | 7928 | 228 | 8295 |
| r1_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 189 | 13137 | 455180 | 30 | 413 | 11732 |
| r1_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 64 | 6058 | 25258 | 246 | 435 | 11710 |
| r1_LF |  | benchmark 1/2: starting | 101 | 18238 | 119 | 156939 | 219 | 8055 |
| r1_LF |  | benchmark 1/2: warmup prompt run | 355 | 9856 | 1160805 | 9 | 380 | 11762 |
| r1_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1489 | 5387 | 283 | 406 | 11740 |
| r1_LF |  | benchmark 1/2: prompt run 2/2 | 5 | 551 | 2405 | 235 | 415 | 11732 |
| r1_LF |  | benchmark 2/2: warmup prompt run | 26 | 2435 | 15806 | 158 | 437 | 11710 |
| r1_LF |  | benchmark 2/2: prompt run 1/2 | 21 | 1777 | 8039 | 226 | 440 | 11706 |
| r1_LF |  | benchmark 2/2: prompt run 2/2 | 19 | 1679 | 6553 | 262 | 442 | 11703 |
| r1_LFB512 |  | benchmark 1/2: starting | 101 | 18247 | 119 | 157013 | 219 | 8062 |
| r1_LFB512 |  | benchmark 1/2: warmup prompt run | 359 | 9924 | 1160566 | 9 | 382 | 11764 |
| r1_LFB512 |  | benchmark 1/2: prompt run 1/2 | 9 | 1520 | 5380 | 289 | 408 | 11738 |
| r1_LFB512 |  | benchmark 1/2: prompt run 2/2 | 9 | 1193 | 7083 | 172 | 417 | 11729 |
| r1_LFB512 |  | benchmark 2/2: warmup prompt run | 22 | 2167 | 7798 | 285 | 435 | 11711 |
| r1_LFB512 |  | benchmark 2/2: prompt run 1/2 | 17 | 1573 | 4561 | 353 | 438 | 11708 |
| r1_LFB512 |  | benchmark 2/2: prompt run 2/2 | 22 | 2095 | 10067 | 213 | 438 | 11705 |
| r2_C-b2048 | pp512 48.94 pp2048 87.46 | start | 112 | 20013 | 2623 | 7813 | 231 | 8300 |
| r2_C-b2048 |  | running pp512 on MoE split (attention on GPU, experts in RAM | 185 | 13121 | 443806 | 30 | 424 | 11721 |
| r2_C-b2048 |  | running pp2048 on MoE split (attention on GPU, experts in RA | 73 | 7022 | 43031 | 167 | 445 | 11701 |
| r2_C-c2048 | pp512 49.60 pp2048 96.71 | start | 112 | 20090 | 2588 | 7949 | 230 | 8277 |
| r2_C-c2048 |  | running pp512 on MoE split (attention on GPU, experts in RAM | 190 | 13188 | 459423 | 29 | 414 | 11731 |
| r2_C-c2048 |  | running pp2048 on MoE split (attention on GPU, experts in RA | 68 | 6352 | 35533 | 183 | 433 | 11713 |
| r2_C-nw | pp512 50.30 pp2048 89.46 | start | 103 | 18500 | 1009 | 18775 | 216 | 7998 |
| r2_C-nw |  | running pp512 on MoE split (attention on GPU, experts in RAM | 376 | 13051 | 1159180 | 12 | 385 | 11759 |
| r2_C-nw |  | running pp2048 on MoE split (attention on GPU, experts in RA | 70 | 6760 | 35048 | 198 | 431 | 11715 |
| r2_C | pp512 49.45 pp2048 87.19 | start | 111 | 19899 | 2511 | 8115 | 227 | 8271 |
| r2_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 191 | 13202 | 456685 | 30 | 412 | 11733 |
| r2_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 72 | 6987 | 41673 | 172 | 431 | 11714 |
| r2_LF |  | benchmark 1/2: starting | 101 | 18249 | 119 | 157035 | 216 | 8064 |
| r2_LF |  | benchmark 1/2: warmup prompt run | 358 | 10035 | 1159156 | 9 | 382 | 11764 |
| r2_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1532 | 5351 | 293 | 416 | 11730 |
| r2_LF |  | benchmark 1/2: prompt run 2/2 | 9 | 1188 | 7182 | 169 | 424 | 11722 |
| r2_LF |  | benchmark 2/2: warmup prompt run | 23 | 2184 | 9402 | 238 | 440 | 11706 |
| r2_LF |  | benchmark 2/2: prompt run 1/2 | 17 | 1511 | 4092 | 378 | 442 | 11704 |
| r2_LF |  | benchmark 2/2: prompt run 2/2 | 18 | 1527 | 4878 | 321 | 443 | 11704 |
| r2_LFB512 |  | benchmark 1/2: starting | 100 | 18243 | 119 | 156979 | 216 | 8061 |
| r2_LFB512 |  | benchmark 1/2: warmup prompt run | 357 | 9799 | 1159199 | 9 | 391 | 11755 |
| r2_LFB512 |  | benchmark 1/2: prompt run 1/2 | 9 | 1500 | 5686 | 270 | 417 | 11729 |
| r2_LFB512 |  | benchmark 1/2: prompt run 2/2 | 9 | 1155 | 7724 | 153 | 426 | 11721 |
| r2_LFB512 |  | benchmark 2/2: warmup prompt run | 24 | 2420 | 11552 | 215 | 438 | 11709 |
| r2_LFB512 |  | benchmark 2/2: prompt run 1/2 | 21 | 1993 | 9617 | 212 | 438 | 11708 |
| r2_LFB512 |  | benchmark 2/2: prompt run 2/2 | 23 | 2042 | 10850 | 193 | 438 | 11706 |
| r3_C-b2048 | pp512 48.57 pp2048 93.08 | start | 112 | 20011 | 2625 | 7806 | 229 | 8306 |
| r3_C-b2048 |  | running pp512 on MoE split (attention on GPU, experts in RAM | 188 | 13275 | 445909 | 30 | 418 | 11718 |
| r3_C-b2048 |  | running pp2048 on MoE split (attention on GPU, experts in RA | 69 | 6380 | 36593 | 179 | 439 | 11706 |
| r3_C-c2048 | pp512 49.32 pp2048 94.04 | start | 111 | 19899 | 2510 | 8118 | 227 | 8273 |
| r3_C-c2048 |  | running pp512 on MoE split (attention on GPU, experts in RAM | 190 | 13354 | 461387 | 30 | 411 | 11734 |
| r3_C-c2048 |  | running pp2048 on MoE split (attention on GPU, experts in RA | 69 | 6498 | 37068 | 179 | 425 | 11715 |
| r3_C-nw | pp512 50.60 pp2048 93.46 | start | 103 | 18495 | 1010 | 18751 | 218 | 7993 |
| r3_C-nw |  | running pp512 on MoE split (attention on GPU, experts in RAM | 375 | 13097 | 1159003 | 12 | 385 | 11761 |
| r3_C-nw |  | running pp2048 on MoE split (attention on GPU, experts in RA | 67 | 6470 | 29946 | 221 | 427 | 11718 |
| r3_C | pp512 49.55 pp2048 92.45 | start | 111 | 19904 | 2517 | 8098 | 230 | 8272 |
| r3_C |  | running pp512 on MoE split (attention on GPU, experts in RAM | 190 | 13209 | 456530 | 30 | 414 | 11731 |
| r3_C |  | running pp2048 on MoE split (attention on GPU, experts in RA | 70 | 6616 | 37278 | 182 | 433 | 11713 |
| r3_LF |  | benchmark 1/2: starting | 101 | 18248 | 119 | 157021 | 216 | 8063 |
| r3_LF |  | benchmark 1/2: warmup prompt run | 358 | 10051 | 1159335 | 9 | 388 | 11757 |
| r3_LF |  | benchmark 1/2: prompt run 1/2 | 9 | 1529 | 5320 | 294 | 416 | 11730 |
| r3_LF |  | benchmark 1/2: prompt run 2/2 | 9 | 1193 | 7262 | 168 | 425 | 11722 |
| r3_LF |  | benchmark 2/2: warmup prompt run | 23 | 2275 | 11206 | 208 | 437 | 11709 |
| r3_LF |  | benchmark 2/2: prompt run 1/2 | 19 | 1758 | 7487 | 240 | 440 | 11707 |
| r3_LF |  | benchmark 2/2: prompt run 2/2 | 19 | 1627 | 6225 | 268 | 441 | 11704 |
| r3_LFB512 |  | benchmark 1/2: starting | 100 | 18248 | 119 | 157025 | 219 | 8065 |
| r3_LFB512 |  | benchmark 1/2: warmup prompt run | 356 | 9935 | 1160179 | 9 | 382 | 11764 |
| r3_LFB512 |  | benchmark 1/2: prompt run 1/2 | 9 | 1516 | 5490 | 283 | 408 | 11738 |
| r3_LFB512 |  | benchmark 1/2: prompt run 2/2 | 7 | 837 | 4962 | 173 | 417 | 11730 |
| r3_LFB512 |  | benchmark 2/2: warmup prompt run | 25 | 2541 | 14163 | 184 | 432 | 11714 |
| r3_LFB512 |  | benchmark 2/2: prompt run 1/2 | 21 | 1901 | 8706 | 224 | 433 | 11713 |
| r3_LFB512 |  | benchmark 2/2: prompt run 2/2 | 22 | 1878 | 7684 | 250 | 434 | 11714 |
