# Run 75: run 71's pp2048 follow-ups, `-p 2048` alone and a page-cache snapshot (2026-11-25, L4 VM)

Follow-up to run 71 (`../2026-11-24-run71-pp2048-cause/SUMMARY.md`, Pending): Cluaiz `bench` pp2048 on the 35B MoE
with experts read from the disk is about 0.86-0.92x of llama-bench, pp512 is equal, and the slower tool reads more
bytes in the pp2048 phase. Run 71 rejected batch size, context size and the load warmup as the cause.

## Setup

| Item | Value |
|---|---|
| Machine | GCP L4 VM (the run 48 VM), kernel 6.12.118 (run 71: 6.12.117; the NVIDIA DKMS module was rebuilt for the new kernel before this run) |
| Binaries | unchanged from run 71: Cluaiz `~/ppcause/bin/cluaiz` (run 71's build) and the llama-bench fork build of runs 48/59/63/71 (build `8676a916e`); md5 of both and of `pp_gap.sh` in `raw/inputs.md5` |
| Model | Qwen3.6 35B A3B MTP UD-IQ4_XS (16.95 GiB, `qwen35moe`), `n_cpu_moe` 41, all layers on the GPU otherwise |
| Conditions | run 71's harness `pp_gap.sh`: VRAM hog holding 14,424 MiB, 12 GB RAM cgroup, page cache dropped before each run, 2 repetitions per test, 3 rounds with the tool order rotated; C = Cluaiz defaults, LF = llama-bench defaults |
| Sets | `p2048`: `-p 2048` alone on both tools (no pp512 phase before it). `p512_2048`: `-p 512,2048` as in run 71 |
| New | `mincore_watch.py` beside each set: a `mincore` snapshot of the model file at the first pp2048 phase mark of each run (`<run>.mincore.json`) |
| Script | `run.sh` (on the VM: `bash run.sh <out root>`); 21:06 to 22:38 UTC |

## Results (`raw/p2048/`, `raw/p512_2048/`; each run's `.md` speed table and `phases.md`)

| Set | Cluaiz pp2048 (3 rounds) | llama-bench pp2048 (3 rounds) | Ratio of means |
|---|---|---|---|
| `p2048` (alone) | 88.13, 93.60, 93.50 (mean 91.74) | 98.26, 96.75, 96.55 (mean 97.19) | 0.944 |
| `p512_2048` (as run 71) | 93.38, 92.11, 97.52 (mean 94.34) | 101.72, 108.78, 100.67 (mean 103.72) | 0.910 |

- pp512 equal again: Cluaiz 51.31-51.90, llama-bench 52.33-52.42.
- Run times: Cluaiz 357-379 s with 0.48-0.49 M major faults per run; llama-bench 543-560 s with 1.19-1.20 M
  (llama-bench's first warmup prompt run pages the model in, as in run 71).
- **Page cache at the pp2048 start (`p512_2048`)**: both tools had the same residency, 11.39-11.43 GB of the
  14.88 GB of experts and 11.62-11.64 GB of the 17.37 GB file. So the gap is not a difference in what was already
  in RAM when pp2048 began.
- **Bytes read in the pp2048 phase (`p512_2048`, `phases.md`)**: Cluaiz 6,165-6,329 MiB in 67-70 s; llama-bench
  about 5.2-5.4 GB (its warmup prompt run 2,058-2,095 MiB plus two prompt runs of 1,490-1,743 MiB each). Same
  direction as run 71 (llama-bench 5,222-5,891 MiB, Cluaiz 5,806-7,022 MiB).
- `p2048` alone: the snapshots are not comparable between the tools. Cluaiz's mark falls after its load warmup
  (9.65 GB of experts resident), llama-bench's after its start (0 resident). The ratio (0.944) is inside run 71's
  range.

## What this proves

- The pp2048 gap repeats on a second day and kernel: 0.910 with run 71's exact settings, 0.944 with `-p 2048` alone.
  So it is not caused by the pp512 phase that ran before pp2048.
- With the same pages resident at the phase start, Cluaiz still reads more from the disk during pp2048 and is
  slower. The cause sits in how the pp2048 phase itself touches expert pages, not in what earlier phases left
  in RAM.

## What this does not prove

- The root cause is still not found. Next steps from run 71's list are still open: a thread pool in `cluaiz bench`,
  and a page-level trace (which expert blocks each tool reads in the phase).
- One model, one machine, 3 rounds per set; it does not show the gap on a model that fits in RAM (runs 63 and 71
  show it only with experts read from the disk).
- These are run 71's binaries, not the current source.

## Files

`run.sh`, `mincore_watch.py`; `raw/steps.txt`, `raw/inputs.md5`, `raw/p2048.out`, `raw/p512_2048.out`; per set:
per run `.md` (speed table), `.log` (time-stamped stderr), `.samples.csv` (1 s sampler), `.time`, `.delta`,
`.mincore.json`, `.done`, plus `steps.log`, `machine.txt`, `mincore_watch.log`, `phases.md`. User name replaced
with `<user>`; no token, VM address or project id in any file.

## Pending

| Item | Needs |
|---|---|
| Root cause of the pp2048 gap | page-level read trace per tool in the pp2048 phase; a thread pool in `cluaiz bench` |
| Same check on the current source | a VM build of the current main |
