## Serve runs (one load per run): generation t/s of the four prompts, prompt t/s of the long prompts

| Run | code | chat | list | long | pp512 | pp2048 | pp4096 | prompt tokens (pp rows) | disk / time |
|---|---|---|---|---|---|---|---|---|---|
| r1_B-serve | 4.68 | 10.13 | 6.33 | 4.97 | 22.33 | 79.49 | 144.62 | 527, 2165, 4404 | rc 0, major faults 5212, read 74675 MiB from disk, 481 s |
| r2_B-serve | 4.50 | 10.13 | 6.29 | 4.87 | 22.14 | 79.68 | 146.32 | 527, 2165, 4404 | rc 0, major faults 5138, read 74908 MiB from disk, 483 s |
| r3_B-serve | 4.51 | 10.09 | 6.28 | 4.82 | 22.14 | 79.52 | 146.55 | 527, 2165, 4404 | rc 0, major faults 5201, read 74989 MiB from disk, 484 s |
| r1_C-pf-serve | 10.18 | 25.11 | 18.91 | 19.88 | 36.52 | 100.70 | 158.75 | 525, 2163, 4402 | rc 0, major faults 32306, read 43855 MiB from disk, 280 s |
| r2_C-pf-serve | 10.21 | 25.24 | 18.83 | 19.76 | 36.04 | 100.64 | 159.39 | 525, 2163, 4402 | rc 0, major faults 33235, read 44069 MiB from disk, 280 s |
| r3_C-pf-serve | 10.21 | 24.91 | 18.76 | 19.63 | 36.86 | 98.39 | 161.07 | 525, 2163, 4402 | rc 0, major faults 32668, read 43842 MiB from disk, 280 s |
| r1_C-serve | 7.67 | 21.34 | 13.46 | 16.38 | 29.14 | 96.40 | 143.79 | 525, 2163, 4402 | rc 0, major faults 113003, read 48006 MiB from disk, 313 s |
| r2_C-serve | 7.74 | 21.76 | 12.99 | 16.05 | 30.13 | 101.71 | 150.91 | 525, 2163, 4402 | rc 0, major faults 109766, read 47737 MiB from disk, 310 s |
| r3_C-serve | 7.86 | 21.13 | 13.44 | 15.71 | 30.41 | 108.90 | 156.21 | 525, 2163, 4402 | rc 0, major faults 104107, read 47023 MiB from disk, 307 s |
| r1_L-serve | 6.17 | 10.94 | 8.73 | 9.12 | 19.65 | 27.89 | 23.93 | 527, 2165, 4404 | rc 0, major faults 277802, read 90797 MiB from disk, 577 s |
| r2_L-serve | 6.17 | 11.16 | 8.74 | 9.09 | 19.82 | 21.22 | 13.30 | 527, 2165, 4404 | rc 0, major faults 276250, read 126095 MiB from disk, 750 s |
| r3_L-serve | 6.17 | 10.97 | 8.59 | 8.99 | 19.64 | 27.97 | 14.34 | 527, 2165, 4404 | rc 0, major faults 278131, read 116755 MiB from disk, 703 s |
| r1_L-serve-mc | 7.69 | 20.02 | 13.20 | 16.11 | 34.43 | 116.45 | 192.47 | 527, 2165, 4404 | rc 0, major faults 108327, read 44921 MiB from disk, 294 s |
| r2_L-serve-mc | 7.63 | 20.81 | 12.84 | 15.93 | 33.73 | 130.96 | 188.17 | 527, 2165, 4404 | rc 0, major faults 104085, read 44840 MiB from disk, 293 s |
| r3_L-serve-mc | 7.63 | 20.70 | 13.15 | 15.57 | 33.31 | 124.06 | 177.21 | 527, 2165, 4404 | rc 0, major faults 107824, read 45234 MiB from disk, 297 s |

### Median (range) of the rounds

| Config | code | chat | list | long | pp512 | pp2048 | pp4096 |
|---|---|---|---|---|---|---|---|
| B-serve | 4.51 (4.50-4.68) | 10.13 (10.09-10.13) | 6.29 (6.28-6.33) | 4.87 (4.82-4.97) | 22.14 (22.14-22.33) | 79.52 (79.49-79.68) | 146.32 (144.62-146.55) |
| C-pf-serve | 10.21 (10.18-10.21) | 25.11 (24.91-25.24) | 18.83 (18.76-18.91) | 19.76 (19.63-19.88) | 36.52 (36.04-36.86) | 100.64 (98.39-100.70) | 159.39 (158.75-161.07) |
| C-serve | 7.74 (7.67-7.86) | 21.34 (21.13-21.76) | 13.44 (12.99-13.46) | 16.05 (15.71-16.38) | 30.13 (29.14-30.41) | 101.71 (96.40-108.90) | 150.91 (143.79-156.21) |
| L-serve | 6.17 (6.17-6.17) | 10.97 (10.94-11.16) | 8.73 (8.59-8.74) | 9.09 (8.99-9.12) | 19.65 (19.64-19.82) | 27.89 (21.22-27.97) | 14.34 (13.30-23.93) |
| L-serve-mc | 7.63 (7.63-7.69) | 20.70 (20.02-20.81) | 13.15 (12.84-13.20) | 15.93 (15.57-16.11) | 33.73 (33.31-34.43) | 124.06 (116.45-130.96) | 188.17 (177.21-192.47) |

## Bench runs (one load per run): t/s

| Run | pp512 | pp2048 | pp4096 | tg128 | disk / time |
|---|---|---|---|---|---|
| r1_B-cli |  |  |  | 2.33 | rc 0, major faults 4442, read 37489 MiB from disk, 241 s |
| r2_B-cli |  |  |  | 2.33 | rc 0, major faults 4425, read 37469 MiB from disk, 241 s |
| r3_B-cli |  |  |  | 2.33 | rc 0, major faults 4431, read 37495 MiB from disk, 242 s |
| r1_C-auto | 45.11 | 91.11 | 94.23 | 26.68 | rc 0, major faults 574343, read 53750 MiB from disk, 528 s |
| r1_C-b2048 | 43.61 | 86.49 | 94.67 | 21.62 | rc 0, major faults 563431, read 53598 MiB from disk, 530 s |
| r2_C-b2048 | 43.91 | 87.59 | 83.53 | 19.62 | rc 0, major faults 582791, read 55595 MiB from disk, 542 s |
| r3_C-b2048 | 44.72 | 87.55 | 91.33 | 19.31 | rc 0, major faults 576862, read 54297 MiB from disk, 531 s |
| r1_C-off | 112.86 | 217.57 | 246.95 | 16.72 | rc 0, major faults 465560, read 29368 MiB from disk, 372 s |
| r2_C-off | 111.22 | 210.40 | 239.35 | 16.91 | rc 0, major faults 463948, read 29244 MiB from disk, 374 s |
| r3_C-off | 114.00 | 215.50 | 245.45 | 16.68 | rc 0, major faults 465479, read 29271 MiB from disk, 372 s |
| r1_C-pf | 44.48 | 96.79 | 81.72 | 26.58 | rc 0, major faults 559468, read 52647 MiB from disk, 529 s |
| r2_C-pf | 45.02 | 88.85 | 94.49 | 26.76 | rc 0, major faults 562168, read 52407 MiB from disk, 526 s |
| r3_C-pf | 44.50 | 90.44 | 88.97 | 24.91 | rc 0, major faults 580056, read 53861 MiB from disk, 535 s |
| r1_C-pin | 44.33 | 85.91 | 92.59 | 10.82 | rc 0, major faults 581857, read 54641 MiB from disk, 548 s |
| r2_C-pin | 43.72 | 85.75 | 82.65 | 10.53 | rc 0, major faults 598406, read 56339 MiB from disk, 558 s |
| r3_C-pin | 46.12 | 94.31 | 88.82 | 10.88 | rc 0, major faults 571001, read 53843 MiB from disk, 535 s |
| r1_C-pin-c4608 | 39.03 | 79.88 | 71.36 | 10.68 | rc 0, major faults 623917, read 65745 MiB from disk, 601 s |
| r2_C-pin-c4608 | 37.95 | 76.19 | 66.58 | 10.60 | rc 0, major faults 623683, read 66392 MiB from disk, 605 s |
| r1_C-plan | 43.63 | 91.24 | 91.71 | 21.39 | rc 0, major faults 576757, read 54261 MiB from disk, 536 s |
| r2_C-plan | 45.16 | 86.91 | 84.28 | 18.87 | rc 0, major faults 600811, read 56292 MiB from disk, 548 s |
| r3_C-plan | 44.33 | 88.28 | 91.09 | 20.87 | rc 0, major faults 572639, read 52869 MiB from disk, 527 s |
| r1_L-bench | 50.37 | 109.43 | 91.62 | 11.40 | rc 0, major faults 1284422, read 51260 MiB from disk, 715 s |
| r2_L-bench | 49.16 | 108.12 | 96.28 | 10.45 | rc 0, major faults 1257849, read 49775 MiB from disk, 706 s |
| r3_L-bench | 50.10 | 106.95 | 86.74 | 10.49 | rc 0, major faults 1280780, read 50938 MiB from disk, 710 s |
| r1_LF-bench | 49.37 | 108.14 | 84.61 | 11.43 | rc 0, major faults 1267051, read 51237 MiB from disk, 711 s |
| r2_LF-bench | 48.73 | 97.65 | 101.34 | 11.14 | rc 0, major faults 1255440, read 50036 MiB from disk, 706 s |
| r3_LF-bench | 49.92 | 111.24 | 88.65 | 10.49 | rc 0, major faults 1277786, read 51198 MiB from disk, 708 s |

### Median (range) of the rounds

| Config | pp512 | pp2048 | pp4096 | tg128 |
|---|---|---|---|---|
| B-cli |  |  |  | 2.33 (2.33-2.33) |
| C-auto | 45.11 (45.11-45.11) | 91.11 (91.11-91.11) | 94.23 (94.23-94.23) | 26.68 (26.68-26.68) |
| C-b2048 | 43.91 (43.61-44.72) | 87.55 (86.49-87.59) | 91.33 (83.53-94.67) | 19.62 (19.31-21.62) |
| C-off | 112.86 (111.22-114.00) | 215.50 (210.40-217.57) | 245.45 (239.35-246.95) | 16.72 (16.68-16.91) |
| C-pf | 44.50 (44.48-45.02) | 90.44 (88.85-96.79) | 88.97 (81.72-94.49) | 26.58 (24.91-26.76) |
| C-pin | 44.33 (43.72-46.12) | 85.91 (85.75-94.31) | 88.82 (82.65-92.59) | 10.82 (10.53-10.88) |
| C-pin-c4608 | 38.49 (37.95-39.03) | 78.03 (76.19-79.88) | 68.97 (66.58-71.36) | 10.64 (10.60-10.68) |
| C-plan | 44.33 (43.63-45.16) | 88.28 (86.91-91.24) | 91.09 (84.28-91.71) | 20.87 (18.87-21.39) |
| L-bench | 50.10 (49.16-50.37) | 108.12 (106.95-109.43) | 91.62 (86.74-96.28) | 10.49 (10.45-11.40) |
| LF-bench | 49.37 (48.73-49.92) | 108.14 (97.65-111.24) | 88.65 (84.61-101.34) | 11.14 (10.49-11.43) |
