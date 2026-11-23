# Run 43: run 23 case G again on the L4 VM, with the MoE cache and the expert prefetch

Date: 2026-11-23. Machine: GCP `g2-standard-8` (<vps>): NVIDIA L4 23 GB, 8 vCPU, 31 GB RAM, Debian
kernel 6.12, driver 620.70.06, NVMe boot disk. Raw files: `results/`.

## Question

Run 23 case G put Qwen3.6-35B-A3B UD-IQ4_XS (18.2 GB, experts 14.53 GB) on this VM with 8 GB of VRAM
left and RAM capped at 12 GB, so part of the experts is read from the SSD again and again. MTP off gave
9.2-13.1 t/s. Does the MoE cache and the new expert prefetch (llama.cpp fork, `CLUAIZ_EXPERT_PREFETCH`)
make this faster, and how does official llama.cpp do on the same setup?

## Setup (same as case G)

- **VRAM limit:** `vram_hog.py 0 8` holds all but 8 GB of the L4 (`results/hog.log`: took 13.90 GB,
  21.90 GB free before). Same script and arguments as run 23.
- **RAM cap:** every run in its own `systemd-run --scope -p MemoryMax=12288M -p MemorySwapMax=0` cgroup
  (page cache counted), as run 23's `limited 12288`. Peak cgroup memory was 12,287-12,288 MiB in every run.
- **Page cache dropped** (`echo 3 > /proc/sys/vm/drop_caches`) before every run.
- Model file: the one run 23 downloaded (`results/model_files.txt`, 18,209,036,576 bytes).
- Prompts: case G's four (`cmd/src/cli/data/bench_mtp_prompts.json`: code, chat, list, long), greedy,
  thinking off, MTP off, 4 threads.
- Cluaiz: release build (`cargo build -p cmd --release`, CUDA arch 89, as run 23) of Engine `60ab3cf1`
  with fork `8676a916e` (unpushed, shipped by `git bundle`).
- Official llama.cpp: `b54752d` (2026-11-15), already on the VM.
- Script: `test/cloud/remote/ssd_prefetch/run_case_g.sh` with
  `CONFIGS='L-text C-plan-text C-cache-text C-pf-text'`, 3 rounds, order rotated each round.
  Tables: `summarize_case_g.py` (`results/results.md`).

| Config | What runs |
|---|---|
| L-text | official `llama-server -ngl 99 -ncmoe 41 -c 16384 -ctk f16 -ctv f16 -fa auto -t 4`, the 4 prompts as `/v1/chat/completions` requests (one load) |
| C-plan-text | `cluaiz run` per prompt (one load per prompt), planner default, prefetch off |
| C-cache-text | the same with `CLUAIZ_MOE_CACHE=on` |
| C-pf-text | the same with `CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=on` |

## Did the MoE cache and the prefetch run? Yes

- Every Cluaiz prompt load logged `CUDA0 MoE cache size =  3602.69 MiB for 14514.00 MiB of host experts,
  90% of the misses on the CPU` (4 lines in each of the 9 Cluaiz logs).
- Every C-pf prompt logged the prefetch counters, the same in all 3 rounds (`results/r*_C-pf-text.log`):
  `8789 calls, 41840 MiB` (code), `5380 calls, 21060 MiB` (chat), `7850 calls, 32330 MiB` (list),
  `4219 calls, 20042 MiB` (long).
- The planner gives the MoE cache by default here (plan: `MoE split ... MoE cache 3.52 GB in VRAM (CPU
  share 0.9)` in `plan_default.log`), so `CLUAIZ_MOE_CACHE=on` changes nothing: C-plan and C-cache
  logged the same cache hit counters (for example `hits = 48702, misses = 26514`). In run 23 case G the
  same plan had no MoE cache (its table header shows only the MoE split). Unlike run 42 (4 GB GPU), 8 GB
  left enough VRAM for a 3.5 GB cache.

## Results (generation t/s, median of 3, range in brackets)

| Config | code | chat | list | long | disk read per run | wall time per run |
|---|---|---|---|---|---|---|
| L-text (official llama.cpp) | 6.01 (5.98-6.14) | 10.88 (10.77-11.11) | 8.43 (8.39-8.54) | 8.88 (8.72-8.93) | 41,368 MiB | 275 s |
| C-plan-text | 7.70 (7.65-7.80) | 11.45 (11.33-11.62) | 9.56 (9.47-9.80) | 14.51 (14.34-14.55) | 102,221 MiB | 584 s |
| C-cache-text | 7.76 (7.71-7.78) | 11.78 (11.16-11.80) | 9.50 (9.28-9.63) | 14.72 (14.44-15.25) | 102,157 MiB | 586 s |
| C-pf-text | **10.21** (10.19-10.21) | **14.93** (14.84-14.99) | **13.55** (13.34-14.15) | **19.52** (19.17-19.70) | 93,215 MiB | 562 s |

| Ratio (median) | code | chat | list | long |
|---|---|---|---|---|
| C-pf / C-cache | 1.32x | 1.27x | 1.43x | 1.33x |
| C-pf / L-text | 1.70x | 1.37x | 1.61x | 2.20x |
| C-plan / L-text | 1.28x | 1.05x | 1.13x | 1.63x |

The rounds do not overlap between C-pf and C-cache on any prompt. Every run per round, raw prompt
speed, peak RSS, VRAM and major faults: `results/results.md`.

Prompt speed: Cluaiz code prompt 1.40 t/s (C-cache) vs 2.37 t/s (C-pf); llama.cpp 1.47 t/s. These are
cold prompts (each Cluaiz prompt is a fresh load), not comparable to case G's prompt column.

## Compared with run 23 case G (MTP off)

| | code | chat | list | long |
|---|---|---|---|---|
| Run 23 G, one `bench --mtp-compare` load for all 4 prompts, no MoE cache | 13.05 | 11.70 | 9.16 | 9.46 |
| Run 43 C-plan-text (one load per prompt, MoE cache on by default) | 7.70 | 11.45 | 9.56 | 14.51 |
| Run 43 C-pf-text | 10.21 | 14.93 | 13.55 | 19.52 |

Not the same method: run 23 G read the prompts in one process, one round, code first; run 43 starts a
new process per prompt with the page cache of earlier prompts still in the cgroup. Code was first in run
23 too, so code being slower now (7.70 vs 13.05) is not explained by order; the cause is not known (?).
The other three prompts are equal or faster. Treat the run 23 row as context, not as a baseline.

## Caveats

- Method differs between the tools: llama-server loads once and answers all 4 prompts; Cluaiz loads once
  per prompt (4 loads). That is why Cluaiz reads about 2.5x more from the disk and takes about 2x the
  wall time per run, mostly loading. The t/s numbers above are per-answer generation speed only.
- Both tools used plain mmap. llama.cpp warns `tensor overrides to CPU are used with mmap enabled -
  consider using --load-mode none`; that mode was not tested.
- One machine, one model, 3 rounds. Text equality between configs was not checked.
- Disk and fault columns come from `/proc/diskstats` and `/proc/vmstat` for the whole VM.

## Pending

| Item | Why |
|---|---|
| Same prompts with one Cluaiz load (server, 4 requests) | removes the load-per-prompt difference against llama-server |
| Why code is slower than run 23 G | not explained; would need a run 23 build next to this one |
| llama.cpp `--load-mode none` and the fork's `--moe-cache-mib` on llama-server | a fairer llama.cpp baseline |
| `cluaiz bench` (pp512 / tg128 / tg400) configs of `run_case_g.sh` | not run, to keep VM time short |
| Text equality check across configs | not done |
| Auto mode for the prefetch | owner decision; this is one positive result on one setup |
