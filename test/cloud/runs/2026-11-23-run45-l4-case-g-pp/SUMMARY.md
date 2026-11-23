# Run 45: prompt processing on the L4, run 23 case G shape, llama.cpp against Cluaiz

Date: 2026-11-23. Same VM, model and limits as run 43 (`cluaiz-32gb`, L4 with about 8 GB left by
`vram_hog.py`, RAM capped at 12 GB by a cgroup, Qwen3.6-35B-A3B UD-IQ4_XS 18.2 GB read from the SSD).
Cluaiz: release build of Engine `60ab3cf1` with fork `8676a916e` (the run 43 build). Official llama.cpp `b54752d`.
Script: `test/cloud/remote/ssd_prefetch/run_case_g.sh` with
`PP=512,2048,4096 TG=0 REPS=2 ROUNDS=3 CONFIGS='L-bench C-plan C-cache C-pf'`. Raw output: `results/`.

## Why this run

Run 43 measured generation only fairly: `llama-server` loaded the model once for its 4 prompts while
Cluaiz loaded it once per prompt, so its prompt speeds were not comparable. Here both tools use their bench
command: one load, one warm-up, then the measured repetitions (llama-bench's default warm-up;
`engine/runtime/src/host.rs:1163` "after one warmup"). Prompt only, no generation.

## Results (prompt t/s, median of 3 rounds, range in brackets)

| Config | pp512 | pp2048 | pp4096 |
|---|---|---|---|
| L: llama-bench `-ngl 99 -ncmoe 41` | 52.03 (52.02-61.61) | 108.21 (102.26-114.81) | 91.27 (89.36-95.21) |
| C-plan: `cluaiz bench`, planner default (MoE cache 3.52 GB) | 46.50 (45.06-47.59) | 94.70 (93.05-100.35) | 93.87 (86.56-109.48) |
| C-cache: + `CLUAIZ_MOE_CACHE=on` | 47.04 (46.68-47.19) | 92.22 (91.07-93.88) | 89.61 (87.02-102.11) |
| C-pf: + `CLUAIZ_EXPERT_PREFETCH=on` | 46.98 (46.53-48.18) | 98.30 (94.76-101.67) | 91.41 (87.99-100.42) |

## What this shows

1. Run 43's "Cluaiz reads prompts 2.5x slower" was the reload per prompt, not the Engine.
2. At 4096 tokens Cluaiz equals llama.cpp (89.6-93.9 against 91.3).
3. At 512 and 2048 tokens Cluaiz is about 9-11% behind (46.5-47.0 against 52.0; 92.2-98.3 against 108.2).
   The cause is not known (?).
4. The expert prefetch neither helps nor hurts prompt processing beyond the round-to-round spread.

## Caveats

- One machine, one model. Standard deviations within a run reach 14 t/s on llama.cpp pp512 round 2.
- Plain mmap on both sides; llama.cpp's `--moe-cache-mib` (fork) was not used on the llama.cpp side.
- The VM booted kernel 6.12.118 without the NVIDIA module; it was booted once into 6.12.117
  (`grub-reboot`) where the driver is installed.

## Pending

| Item | Why |
|---|---|
| pp512 / pp2048 gap of about 10% | find the cause (same family as the warm tg gap on the laptop, run 42) |
