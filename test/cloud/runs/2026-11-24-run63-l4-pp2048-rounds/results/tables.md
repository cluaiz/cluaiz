| round | tool | pp512 | pp2048 | run totals |
|---|---|---|---|---|
| 1 | C | 49.93 ± 0.51 | 88.81 ± 0.92 | rc 0, major faults 498828, read 40421 MiB from disk, 369 s |
| 1 | LF | 50.47 ± 0.11 | 108.95 ± 0.69 | rc 0, major faults 1194738, read 37006 MiB from disk, 537 s |
| 2 | C | 51.22 ± 2.60 | 100.89 ± 0.91 | rc 0, major faults 486726, read 39592 MiB from disk, 365 s |
| 2 | LF | 51.33 ± 0.27 | 95.54 ± 0.59 | rc 0, major faults 1207609, read 38082 MiB from disk, 541 s |
| 3 | C | 50.73 ± 2.66 | 97.94 ± 1.40 | rc 0, major faults 487010, read 39726 MiB from disk, 365 s |
| 3 | LF | 60.30 ± 12.78 | 101.46 ± 4.20 | rc 0, major faults 1203453, read 37376 MiB from disk, 542 s |
| 4 | C | 48.81 ± 0.98 | 88.17 ± 0.11 | rc 0, major faults 503277, read 40560 MiB from disk, 374 s |
| 4 | LF | 50.91 ± 0.16 | 108.92 ± 0.16 | rc 0, major faults 1194309, read 37012 MiB from disk, 538 s |
| 5 | C | 49.44 ± 0.22 | 90.18 ± 5.41 | rc 0, major faults 497329, read 40219 MiB from disk, 372 s |
| 5 | LF | 66.70 ± 18.53 | 105.11 ± 4.90 | rc 0, major faults 1200925, read 36981 MiB from disk, 538 s |

| test | Cluaiz median (range) | llama-bench median (range) | ratio per round | ratio median (range) |
|---|---|---|---|---|
| pp512 | 49.93 (48.81-51.22) | 51.33 (50.47-66.70) | 0.989, 0.998, 0.841, 0.959, 0.741 | 0.959 (0.741-0.998) |
| pp2048 | 90.18 (88.17-100.89) | 105.11 (95.54-108.95) | 0.815, 1.056, 0.965, 0.809, 0.858 | 0.858 (0.809-1.056) |
