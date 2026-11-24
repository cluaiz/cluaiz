| round | config | pp512 | pp2048 | pp2048 phase MiB read | run totals |
|---|---|---|---|---|---|
| 1 | LF | 67.53 ± 22.14 | 100.39 ± 3.55 | 5891 | rc 0, major faults 1203565, read 37018 MiB from disk, 543 s |
| 1 | C | 49.86 ± 2.81 | 101.48 ± 2.60 | 6058 | rc 0, major faults 485816, read 39596 MiB from disk, 367 s |
| 1 | C-b2048 | 50.53 ± 3.08 | 107.26 ± 8.20 | 5806 | rc 0, major faults 472069, read 39251 MiB from disk, 363 s |
| 1 | C-c2048 | 50.42 ± 2.94 | 100.36 ± 0.84 | 6052 | rc 0, major faults 492108, read 39534 MiB from disk, 369 s |
| 1 | C-nw | 50.89 ± 1.53 | 90.48 ± 1.64 | 6622 | rc 0, major faults 1193477, read 38632 MiB from disk, 550 s |
| 1 | LFB512 | 50.57 ± 0.59 | 102.28 ± 10.16 | 5835 | rc 0, major faults 1200636, read 37572 MiB from disk, 546 s |
| 2 | LF | 50.35 ± 0.40 | 110.48 ± 4.15 | 5222 | rc 0, major faults 1194939, read 37103 MiB from disk, 543 s |
| 2 | C | 49.45 ± 0.31 | 87.19 ± 2.75 | 6987 | rc 0, major faults 505109, read 40454 MiB from disk, 377 s |
| 2 | C-b2048 | 48.94 ± 0.57 | 87.46 ± 4.23 | 7022 | rc 0, major faults 490511, read 40391 MiB from disk, 372 s |
| 2 | C-c2048 | 49.60 ± 1.20 | 96.71 ± 2.55 | 6352 | rc 0, major faults 499962, read 39894 MiB from disk, 372 s |
| 2 | C-nw | 50.30 ± 0.86 | 89.46 ± 0.09 | 6760 | rc 0, major faults 1196716, read 38644 MiB from disk, 551 s |
| 2 | LFB512 | 50.21 ± 0.90 | 91.30 ± 1.58 | 6455 | rc 0, major faults 1208670, read 38129 MiB from disk, 551 s |
| 3 | LF | 50.28 ± 0.48 | 101.08 ± 2.36 | 5660 | rc 0, major faults 1202241, read 37542 MiB from disk, 546 s |
| 3 | C | 49.55 ± 0.19 | 92.45 ± 6.64 | 6616 | rc 0, major faults 497674, read 40069 MiB from disk, 373 s |
| 3 | C-b2048 | 48.57 ± 0.22 | 93.08 ± 1.77 | 6380 | rc 0, major faults 486797, read 39905 MiB from disk, 371 s |
| 3 | C-c2048 | 49.32 ± 0.64 | 94.04 ± 1.31 | 6498 | rc 0, major faults 501755, read 39996 MiB from disk, 371 s |
| 3 | C-nw | 50.60 ± 0.13 | 93.46 ± 3.86 | 6470 | rc 0, major faults 1191671, read 38380 MiB from disk, 548 s |
| 3 | LFB512 | 57.29 ± 8.19 | 96.72 ± 3.67 | 6320 | rc 0, major faults 1205412, read 37754 MiB from disk, 547 s |

| config | pp512 median (range) | pp2048 median (range) | pp512 / LF per round | pp2048 / LF per round | pp2048 / LF median | pp2048 MiB read median |
|---|---|---|---|---|---|---|
| LF | 50.35 (50.28-67.53) | 101.08 (100.39-110.48) | 1.000, 1.000, 1.000 | 1.000, 1.000, 1.000 | 1.000 | 5660 |
| C | 49.55 (49.45-49.86) | 92.45 (87.19-101.48) | 0.738, 0.982, 0.985 | 1.011, 0.789, 0.915 | 0.915 | 6616 |
| C-b2048 | 48.94 (48.57-50.53) | 93.08 (87.46-107.26) | 0.748, 0.972, 0.966 | 1.068, 0.792, 0.921 | 0.921 | 6380 |
| C-c2048 | 49.60 (49.32-50.42) | 96.71 (94.04-100.36) | 0.747, 0.985, 0.981 | 1.000, 0.875, 0.930 | 0.930 | 6352 |
| C-nw | 50.60 (50.30-50.89) | 90.48 (89.46-93.46) | 0.754, 0.999, 1.006 | 0.901, 0.810, 0.925 | 0.901 | 6622 |
| LFB512 | 50.57 (50.21-57.29) | 96.72 (91.30-102.28) | 0.749, 0.997, 1.139 | 1.019, 0.826, 0.957 | 0.957 | 6320 |
