# Run 61: load-prefetch skip for CPU-mapped weights (Phase A) on the Windows laptop, model larger than free RAM

Date: 2026-11-24. Script: `test/cloud/remote/ssd_prefetch/run_prefetch_laptop.ps1 -Set lazy` (rotated order, one
load per config per round, typeperf every second), tables by `analyze_lazy_load.py`. Raw output: `results/`.

## Question

At load, llama.cpp asks the OS to read the whole model file (Windows `PrefetchVirtualMemory`). When the model
does not fit in free RAM, most of that is evicted again. Does leaving the CPU-mapped weights (the experts) out
of that prefetch (`CLUAIZ_LAZY_HOST_WEIGHTS=on`, fork patch 3, `engine/plan/src/load_prefetch.rs`) cut load time
and disk reads without costing prompt (pp) or generation (tg) speed?

## Setup

| | |
|---|---|
| Machine | Windows 11 build 26300, Ryzen 7 7435HS (8 cores), 23.7 GB RAM, RTX 3050 Laptop 4 GB (driver 610.47) |
| Other programs | the owner's apps ran throughout: free RAM before each run 7,967-13,944 MiB, lowest free RAM during a run 0-1,680 MiB (per-run columns below); 891-1,047 MiB of VRAM used by others at the start |
| Model | Gemma 4 26B A4B IQ4_XS, 14,205,004,928 bytes (13.21 GiB): larger than free RAM in every run, so the plan reads experts from disk (`1.59 GB over the RAM limit is read from disk when used`) |
| Cluaiz | `cargo build -p cmd --release` of the working tree on `c9fcf0f6` + fork `8676a916e` with patch 3 and the ask-once prefetch (not committed at run time) |
| llama.cpp | official `llama-bench` b12428 (`b4c0e00a4`), CUDA build |
| Flags | `-p 512 -n 128 -r 1 -t 8`, KV f16, flash attention on, `-c 1024` (Cluaiz); L: `-ngl 99 -ncmoe 30 -fa 1`; C / C-lazy: `--n-gpu-layers all --n-cpu-moe 30` (the same placement as L); C-cache / C-pf / C-lazy-pf: the planner's placement with `CLUAIZ_MOE_CACHE=on` |
| Home | a new isolated `CLUAIZ_HOME` per invocation under `%LOCALAPPDATA%\cluaiz-test`; no server, no port |
| Load window | the Engine's `MEMORY TRACE: 1. BEFORE MODEL LOAD` and `2. AFTER MODEL LOAD` lines (`-vv`); disk GB in load = typeperf disk read bytes in that window (+-1 s) |

Two invocations: `matrix` (6 configs, 3 rounds, 08:36-09:01) and `pairs` (C and C-lazy only, 6 rounds,
09:10-09:44). A first invocation with `-r 3` (`results/aborted-reps3/`) was stopped after one llama-bench run
and part of one Cluaiz run: a pp512 pass took 11 minutes at 0.2-1 GB free RAM.

## Results: same placement (C against C-lazy), 9 pairs

| Invocation, round | free RAM before C / C-lazy MiB | load s C / C-lazy | disk GB in load C / C-lazy | disk GB whole run C / C-lazy | pp512 C / C-lazy | tg128 C / C-lazy |
|---|---|---|---|---|---|---|
| matrix r1 | 10,270 / 11,250 | 11.2 / 4.3 | 2.93 / 1.14 | 9.14 / 12.25 | 52.51 / 53.54 | 12.98 / 12.47 |
| matrix r2 | 12,810 / 12,769 | 7.9 / 2.5 | 2.84 / 0.06 | 12.46 / 0.40 | 80.71 / 278.32 | 17.19 / 19.06 |
| matrix r3 | 13,044 / 11,739 | 8.4 / 9.4 | 2.90 / 0.80 | 12.70 / 11.00 | 91.87 / 104.43 | 17.36 / 15.54 |
| pairs r1 | 12,819 / 11,241 | 7.7 / 5.4 | 1.83 / 1.57 | 13.42 / 36.74 | 52.90 / **4.53** | 3.40 / 9.93 |
| pairs r2 | 12,036 / 8,128 | 6.5 / 5.1 | 2.90 / 1.15 | 12.30 / 25.65 | 57.18 / **7.63** | 15.77 / 14.06 |
| pairs r3 | 13,244 / 12,851 | 8.5 / 2.5 | 2.91 / 0.03 | 14.08 / 0.23 | 75.42 / 271.11 | 15.17 / 19.92 |
| pairs r4 | 12,873 / 12,844 | 7.4 / 2.5 | 2.46 / 0.03 | 12.07 / 0.14 | 91.69 / 262.57 | 17.14 / 18.06 |
| pairs r5 | 12,929 / 13,944 | 8.1 / 8.0 | 2.92 / 2.55 | 12.62 / 12.19 | 81.31 / 75.21 | 14.18 / 15.03 |
| pairs r6 | 13,596 / 13,573 | 8.0 / 2.5 | 2.39 / 0.03 | 11.75 / 0.07 | 94.90 / 259.14 | 17.89 / 18.40 |
| **median of 9** | | **8.0 / 4.3** | **2.90 / 0.80** | **12.46 / 11.00** | **80.71 / 104.43** | **15.77 / 15.54** |

Lowest free RAM during the C-lazy runs of pairs r1 and r2: 0 MiB (C in the same rounds: 135 and 1,018 MiB).

## Results: planner placement, MoE cache asked for (matrix, 3 rounds each)

| Config | load s | disk GB in load | pp512 | tg128 |
|---|---|---|---|---|
| C-cache (no skip) | 14.0 (8.8-14.6) | 2.68 (2.12-2.90) | 76.50 (53.30-76.64) | 13.02 (12.53-17.88) |
| C-pf (no skip) | 8.8 (6.9-11.2) | 2.52 (2.51-2.59) | 68.99 (19.55-80.80) | 7.49 (6.12-14.77) |
| C-lazy-pf (skip on) | 3.0 (2.6-3.5) | 0.78 (0.07-0.88) | 110.33 (24.76-243.80) | 16.61 (3.90-17.42) |
| L (llama-bench, `-ncmoe 30`) | - | - | 35.89 (13.99-124.94) | 7.50 (1.99-16.72) |

No run of these three configs got a MoE cache (no `MoE cache size` line; usable VRAM 2.15 GB, the plan was a
MoE partial split with 24-30 blocks on the GPU), so the expert prefetch never ran; the three are the planner's
placement with and without the load skip, placements differing by a few blocks between runs.

## What this shows

1. Load: the skip worked as designed. The fork's line `file 0: 11797 MiB of CPU-mapped weights left out of the
   load prefetch` appears in every C-lazy run (all experts). Median load time 8.0 -> 4.3 s (9 pairs; faster in
   7, equal in 1, slower in 1) and disk read during the load 2.90 -> 0.80 GB. With the planner's placement:
   load 3.0 s against 8.8-14.0 s.
2. Across runs: when free RAM stayed above the experts' working set, C-lazy read almost nothing from disk in
   the whole run (0.07-0.40 GB in 4 of 9 pairs) while C read 9.14-14.08 GB every time: the whole-file prefetch
   (14.2 GB into 10-13 GB of free RAM) evicts its own pages, so each load reads the file again, while the
   skip leaves the pages actually used in the OS cache from the run before.
3. Risk: when free RAM fell to 0 MiB (pairs r1 and r2), C-lazy's prompt pass was 7-12x slower than C's
   (4.53 and 7.63 against 52.90 and 57.18 t/s) and read 26-37 GB in the run: without the load prefetch the
   experts are read by page faults, small reads that the eviction then repeats. C's single large prefetch at
   load did better in that state.
4. Generation: median 15.77 (C) against 15.54 (C-lazy) t/s; the ranges overlap (3.40-17.89 and 9.93-19.92).
   Not lower beyond the noise, not proven equal at +-3% either.
5. Official llama.cpp (L, same placement as C) was the slowest and the noisiest here (tg 1.99-16.72 t/s); the
   machine was too busy for a fair Cluaiz against llama.cpp number in this run.

## Decision

The skip is not turned on by default (Auto stays off, `engine/plan/src/load_prefetch.rs`): it halves load time
and cuts load disk reads by about 2 GB, but in 2 of 9 pairs, at 0 MiB free RAM, prompt processing was 7-12x
slower. The switch `CLUAIZ_LAZY_HOST_WEIGHTS=on` (only for plans that read from disk) is kept for further runs.

## Rerun

```
powershell -ExecutionPolicy Bypass -File test\cloud\remote\ssd_prefetch\run_prefetch_laptop.ps1 -Set lazy -Only L,C,C-lazy,C-cache,C-pf,C-lazy-pf -Reps 1 -Cluaiz target\release\cluaiz.exe -Out <folder>
powershell -ExecutionPolicy Bypass -File test\cloud\remote\ssd_prefetch\run_prefetch_laptop.ps1 -Set lazy -Only C,C-lazy -Rounds 6 -Reps 1 -Cluaiz target\release\cluaiz.exe -Out <folder>
python test\cloud\remote\ssd_prefetch\analyze_lazy_load.py <folder>
```

## Caveats

- A shared laptop under changing load: free RAM before a run moved between 8.0 and 13.9 GB, and another agent's
  CUDA build ran on the machine until 08:32 (the matrix started at 08:36). Single repetitions (`-r 1`).
- Windows cannot drop its file cache without an extra tool, so a run inherits what the run before left in RAM
  (the rotated order spreads this, it does not remove it).
- Load seconds come from the Engine's log lines and are not available for llama-bench.

## Pending

| Item | Why |
|---|---|
| The skip together with the ask-once expert prefetch, with a MoE cache | the prefetch reads first-used experts in large requests (run 60: 1.8x on cold pages), which could remove the 0 MiB free RAM slowdown; needs free VRAM for a cache |
| A free-RAM guard (skip only when free RAM covers the experts used per prompt) | the two bad pairs had 0 MiB free; the planner knows free RAM but not the per-prompt working set |
| Linux | on Linux the lazy ranges also get `POSIX_MADV_RANDOM` (no read-around on faults); not measured |
| A quiet-machine repeat with llama-bench | for a Cluaiz against llama.cpp number |
