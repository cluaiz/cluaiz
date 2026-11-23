# Run 44: run 42 repeated with little free RAM, so the experts are read from the SSD

Date: 2026-11-23. Same laptop, model, script and build as run 42 (`run_prefetch_laptop.ps1`, debug build at `da22cab3`).
Difference: the owner filled about 13 GB of RAM with another program and closed desktop apps to free VRAM, so
free RAM during the runs was 0.06-1.9 GB and every run read 0.7-18 GB from disk. Raw output: `results/`.

## Results (tg128 t/s, median of 3 rounds, range in brackets)

| Config | tg t/s | pp t/s | MoE cache | prefetch |
|---|---|---|---|---|
| L (llama-bench b12428) | 15.73 (12.89-17.16) | 140.97 | - | - |
| C (run 20 placement) | 15.97 (14.78-16.79) | 188.85 | - | - |
| C-plan | 17.39 (15.92-18.39) | 174.55 | - | - |
| C-cache | 17.48 (15.72-20.09) | 172.20 | 402 and 273 MiB in rounds 1-2, none in round 3 | - |
| C-pf | 10.23 (8.68-14.98) | 177.23 | 452 and 258 MiB in rounds 1-2, none in round 3 | 11,548 and 11,554 calls, 173,143 and 215,027 MiB asked, in rounds 1-2 |

## What this shows

1. With the experts read from the SSD, Cluaiz matches llama.cpp: C 15.97 against L 15.73 t/s.
2. With little free RAM the planner did give a small MoE cache (258-452 MiB of VRAM) in 4 of 6 cache runs.
3. **The expert prefetch made generation slower.** In the two runs where it ran: 10.23 and 8.68 t/s, against
   20.09 and 17.48 t/s for the cache alone in the same rounds. It asked the OS to read 173-215 GB for
   about 128+ generated tokens and the prompt runs, far more than the 11.5 GB of experts: it prefetches too much and
   competes with the reads that are really needed. In round 3, with no cache and no prefetch, C-pf gave 14.98.
4. The cache alone was the fastest median (17.48), but its range overlaps C-plan (17.39); not a proven gain.

## Caveats

- Three rounds on a busy laptop; free RAM changed between runs (0.06-1.9 GB). Debug build. No release build.
- The cloud repeat of run 23 case G (run 43) is the cleaner test.

## Pending

| Item | Why |
|---|---|
| Prefetch reads far too much | 173-215 GB asked per run; needs a cap per step or a smarter window before it can be turned on |
| Prefetch stays off by default | it is slower in every run where it ran |
