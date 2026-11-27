# Run 75: pp2048 follow-up, MoE split vs llama-bench

Date: 2026-11-25. Machine: cluaiz-32gb (8 vCPU, 32 GB RAM, no swap, NVIDIA L4 23 GB, driver 620.70.06).
Model: Qwen3.5-35B-A3B IQ4_XS (16.95 GiB). Cluaiz = MoE split (attention on GPU, experts in RAM).
Reference = llama-bench, CUDA, ngl 99, n-cpu-moe 41 (`LF` in file names).

## Prompt speed (tokens/s, from `raw/*/r*_*.md.txt`)

| Test | Cluaiz r1 / r2 / r3 | llama-bench r1 / r2 / r3 |
|---|---|---|
| pp2048 alone | 88.13 / 93.60 / 93.50 | 98.26 / 96.75 / 96.55 |
| pp512 (pp512+2048 set) | 51.90 / 51.53 / ? | 52.42 / 52.36 / ? |
| pp2048 (pp512+2048 set) | 93.38 / 92.11 / ? | 101.72 / ? / ? |

`?` = not read into this summary; values are in `raw/p512_2048/`.

## Disk reads (`raw/p2048/phases.md.txt`, `raw/p512_2048.out`)

- Cluaiz start phase about 111 s; whole r3 run 376 s, 39,408 MiB read, 482,465 major faults.
- llama-bench warmup about 398 s; whole r3 run 560 s, 37,100 MiB read, 1,195,826 major faults.

## Reading

pp2048 is 3-8% slower than llama-bench on this machine; pp512 is equal. Total run time is shorter for
Cluaiz because of the shorter warm-up. Not repeated after 2026-11-25 (needs cluaiz-32gb).
