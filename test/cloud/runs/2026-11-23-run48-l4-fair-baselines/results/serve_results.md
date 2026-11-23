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

### Median (range) of the rounds

| Config | code | chat | list | long | pp512 | pp2048 | pp4096 |
|---|---|---|---|---|---|---|---|
| B-serve | 4.51 (4.50-4.68) | 10.13 (10.09-10.13) | 6.29 (6.28-6.33) | 4.87 (4.82-4.97) | 22.14 (22.14-22.33) | 79.52 (79.49-79.68) | 146.32 (144.62-146.55) |
| C-pf-serve | 10.21 (10.18-10.21) | 25.11 (24.91-25.24) | 18.83 (18.76-18.91) | 19.76 (19.63-19.88) | 36.52 (36.04-36.86) | 100.64 (98.39-100.70) | 159.39 (158.75-161.07) |
| C-serve | 7.74 (7.67-7.86) | 21.34 (21.13-21.76) | 13.44 (12.99-13.46) | 16.05 (15.71-16.38) | 30.13 (29.14-30.41) | 101.71 (96.40-108.90) | 150.91 (143.79-156.21) |
| L-serve | 6.17 (6.17-6.17) | 10.97 (10.94-11.16) | 8.73 (8.59-8.74) | 9.09 (8.99-9.12) | 19.65 (19.64-19.82) | 27.89 (21.22-27.97) | 14.34 (13.30-23.93) |

## Bench runs (one load per run): t/s

| Run | pp512 | pp2048 | pp4096 | tg128 | disk / time |
|---|---|---|---|---|---|
| r1_L-serve-mc |  |  |  |  | rc 0, major faults 108327, read 44921 MiB from disk, 294 s |
| r2_L-serve-mc |  |  |  |  | rc 0, major faults 104085, read 44840 MiB from disk, 293 s |
| r3_L-serve-mc |  |  |  |  | rc 0, major faults 107824, read 45234 MiB from disk, 297 s |

### Median (range) of the rounds

| Config | pp512 | pp2048 | pp4096 | tg128 |
|---|---|---|---|---|
| L-serve-mc |  |  |  |  |
