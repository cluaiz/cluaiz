# Run 63: five more pp2048 rounds on run 59's shape (is the pp2048 gap real?)

Date: 2026-11-24 (09:02 to 10:19 UTC). Follow-up to run 59 (`../2026-11-24-run59-l4-pp-gap/SUMMARY.md`), whose
pending table said "pp2048 gap of 5-10%, if it is real: needs 4+ rounds per config".

Files: `run63.sh` (the exact command), `build63.sh` (the Engine build on the VM), `summarize_run63.py` (tables below),
`results/` (raw: per run `.md` speed table, `.log` time-stamped stderr, `.samples.csv` 1 s sampler, `.time`,
`.delta`; `steps.log`, `run63.out`, `machine.txt`, `build63.log`, `phases.md` from `analyze_pp_gap.py`,
`tables.md` from `summarize_run63.py`). User name replaced by `<user>`.

## Setup (same as run 59 part e, except the Cluaiz binary)

| Item | Value |
|---|---|
| Machine | GCP `g2-standard-8` standard VM (<vps>): NVIDIA L4 23 GB, 8 vCPU, 31 GB RAM, Debian 13, kernel 6.12.117 (6.12.118 booted without the NVIDIA module again, so `grub-reboot "1>2"` into 6.12.117, as run 59), driver 620.70.06 |
| VRAM | `vram_hog.py 0 8`: 14,424 MiB used before the runs (run 59: 14,424), about 8 GB left |
| RAM | each run in its own `systemd-run --scope -p MemoryMax=12288M -p MemorySwapMax=0` cgroup, page cache dropped before each run |
| Model | Qwen3.6-35B-A3B UD-IQ4_XS (run 48 / 59 file) |
| Placement | llama-bench `-ngl 99 -ncmoe 41 -ctk f16 -ctv f16 -fa auto -t 4`; `cluaiz bench --n-gpu-layers all --n-cpu-moe 41 -c 16384 --cache-type-k f16 --cache-type-v f16 --flash-attn auto -t 4`, `CLUAIZ_EXPERT_PREFETCH=off`; no MoE cache on either side |
| MTP | `CLUAIZ_MTP=off` (set by `pp_gap.sh`; with this binary the value is read, commit `d7ca5adf`). Every Cluaiz log also has `[MTP] Auto: off (measured slower on this placement)`; no MTP context was loaded |
| Tests | `-p 512,2048 -r 2 -n 0`, one load per run, 5 rounds, order rotated (rounds 1, 3, 5: Cluaiz first; 2, 4: llama-bench first) |
| llama-bench | the fork build of runs 48 and 59 (`8676a916e`), not rebuilt |
| Cluaiz | release build on the VM of Engine HEAD `9dfd4f46` with fork `96b0cfb2d` (git bundles of the local commits, `build63.sh`, 282 s). It contains `370cf8ac` (bench tokens from `rand()`, run 59's fix). Fork changes since `8676a916e`: the load-prefetch skip (off by default) and the Windows ask-once prefetch (Windows only) |
| Scripts | `test/cloud/remote/ssd_prefetch/pp_gap.sh`, `balloon.py`, `analyze_pp_gap.py` at this commit, md5 checked equal on the VM |

## Results

Each cell is the tool's own mean of its 2 repetitions (± its own spread).

| Round | Cluaiz pp512 | llama-bench pp512 | Cluaiz pp2048 | llama-bench pp2048 | pp2048 ratio |
|---|---|---|---|---|---|
| 1 | 49.93 ± 0.51 | 50.47 ± 0.11 | 88.81 ± 0.92 | 108.95 ± 0.69 | 0.815 |
| 2 | 51.22 ± 2.60 | 51.33 ± 0.27 | 100.89 ± 0.91 | 95.54 ± 0.59 | 1.056 |
| 3 | 50.73 ± 2.66 | 60.30 ± 12.78 | 97.94 ± 1.40 | 101.46 ± 4.20 | 0.965 |
| 4 | 48.81 ± 0.98 | 50.91 ± 0.16 | 88.17 ± 0.11 | 108.92 ± 0.16 | 0.809 |
| 5 | 49.44 ± 0.22 | 66.70 ± 18.53 | 90.18 ± 5.41 | 105.11 ± 4.90 | 0.858 |

| Test | Cluaiz median (range) | llama-bench median (range) | Ratio median (range) |
|---|---|---|---|
| pp512 | 49.93 (48.81-51.22) | 51.33 (50.47-66.70) | 0.959 (0.741-0.998) |
| pp2048 | 90.18 (88.17-100.89) | 105.11 (95.54-108.95) | 0.858 (0.809-1.056) |

**pp512 sanity check against run 59.** Cluaiz 48.81-51.22 here against 49.56 / 50.61 in run 59 part e (after the
fix); llama-bench 50.47-51.33 in the three rounds without a wide spread, against 50.76 / 51.13. The same as run 59.
Rounds 3 and 5 of llama-bench have one fast repetition (± 12.78, ± 18.53), as run 59 saw once (65.18 ± 17.11).

**pp2048.** Cluaiz was slower in 4 of 5 rounds (0.81-0.97x) and faster in 1 (1.06x). The ranges overlap
(Cluaiz up to 100.89, llama-bench down to 95.54), but the medians differ by 15 t/s (0.86x). A Mann-Whitney U test on
the 5 against 5 round means gives U = 2, two-sided exact p = 0.032 (computed by enumerating all 252 splits). With
run 59 part e added (Cluaiz 92.03 / 86.37, llama-bench 94.83 / 99.59), Cluaiz is slower in 6 of 7 rounds.
So the pp2048 gap is more likely real than noise on this shape, at about 10-15% by median, with a wide spread.

**The bytes read follow the speed, as in run 59** (`results/phases.md`, the pp2048 phase = its warmup plus 2 runs):

| Round | Cluaiz pp2048 MiB read | Cluaiz pp2048 t/s | llama-bench pp2048 MiB read | llama-bench pp2048 t/s |
|---|---|---|---|---|
| 1 | 6,959 | 88.81 | 5,226 | 108.95 |
| 2 | 6,066 | 100.89 | 6,315 | 95.54 |
| 3 | 6,348 | 97.94 | 5,982 | 101.46 |
| 4 | 6,972 | 88.17 | 5,120 | 108.92 |
| 5 | 6,800 | 90.18 | 5,739 | 105.11 |

In every round the faster tool read fewer bytes in that phase. Cluaiz read 6.1-7.0 GB, llama-bench 5.1-6.3 GB. Over a
whole run Cluaiz read 39.6-40.6 GB, llama-bench 37.0-38.1 GB; Cluaiz had about 0.49-0.50 M major faults per run against
1.19-1.21 M (its load-time warmup, run 59 section 3), and its whole run took 365-374 s against 537-542 s.

## What this does not prove

- The cause of the pp2048 gap. The token stream is now the same as llama-bench's (`370cf8ac`), so the extra 0.5-1.7 GB
  read in the pp2048 phase comes from something else. Candidates not tested here: the load-time warmup (run 59 part c:
  skipping it moved pp2048 from 90-97 to 91-96, 2 rounds, not decided), the context size (16384 against llama-bench's
  2048), and what the page cache holds after the pp512 phase.
- Anything outside this shape (experts larger than the 12 GB RAM cap, read from the SSD). With the model in RAM the
  bytes read cannot differ.
- 5 rounds of 2 repetitions per tool on one VM. The test above treats each round mean as one sample.
- The Cluaiz binary is newer than run 59's (`9dfd4f46` against `03de2c96` + fix). The pp512 numbers match run 59, but
  a small change between the two binaries cannot be ruled out from this run alone.
- tg128 was not measured (`-n 0`, as run 59 parts a-d).

## Pending

| Item | Why |
|---|---|
| Cause of the remaining pp2048 gap (0.86x median, 6 of 7 rounds slower) | Cluaiz reads 0.5-1.7 GB more in that phase; the warmup and the 16384 context are untested candidates on pp2048 with 4+ rounds |
