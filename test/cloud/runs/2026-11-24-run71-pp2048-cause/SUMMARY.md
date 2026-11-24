# Run 71: which parameter causes the pp2048 gap of run 63? (none of the four tested)

Date: 2026-11-24 (13:59 to 16:17 UTC A/B, 16:30 UTC MTP check). Follow-up to run 63
(`../2026-11-24-run63-l4-pp2048-rounds/SUMMARY.md`): `cluaiz bench` pp2048 at 0.86x of llama-bench, pp512 equal,
and the slower tool read more bytes in the pp2048 phase.

Files: `buildcause.sh` (build on the VM), `nowarm.patch` (test-only switch, see below), `runcause.sh` (the exact
command), `waitcause.sh` (waited for run 66 to end, then built and started), `mtpcheck.sh` (MTP check),
`summarize_cause.py` (tables), `results/` (per run `.md` speed table, `.log` time-stamped stderr, `.samples.csv`
1 s sampler, `.time`, `.delta`; `steps.log`, `run.out`, `machine.txt`, `hog.log`, `build.log`, `phases.md` from
`analyze_pp_gap.py`, `tables.md` from `summarize_cause.py`, `mtp/`). User name replaced by `<user>`.
`results/wait.log` is from the first start, which failed (below); the counted run was started by hand at 13:59 UTC.

## 1. Every parameter that differs for a pp2048 test (code read on the laptop)

| Parameter | llama-bench (fork `tools/llama-bench/llama-bench.cpp`) | `cluaiz bench` | Same? |
|---|---|---|---|
| n_ctx | n_prompt + n_gen per test, a new context per test (`:1311`, `:2383`) | 16384, one context for every test (`engine/runtime/src/native_load.rs`) | no |
| n_batch | 2048 (`:391`); pp2048 is one `llama_process` call of 2048 tokens (`:2182-2190`) | 512 (`engine/core/src/settings.rs:547`); pp2048 is 4 `llama_decode` calls of 512 (`engine/llama/src/native/core.rs:300-321`) | no |
| n_ubatch | 512 (`:392`) | 512 (`settings.rs:552`) | yes |
| warmup | one prompt run per test (`:2418-2430`) | the same per test (`core.rs:338-340`) plus one decode of 512 BOS tokens at load (`core.rs:929`, `:939-968`) | no |
| thread pool | its own per test, poll 50 (`:2397-2416`, `:398`) | none attached (ggml makes one per graph compute) | no |
| n_threads / n_threads_batch | 4 / 4 (`llama_set_n_threads`, `:2172`) | 4 / 4 (logs) | yes |
| flash attention, KV type, offload_kqv, op_offload, swa_full | auto, f16, 1, 1, false (`:1314-1320`) | the same (`native_load.rs`) | yes |
| mmap / madvise | load mode AUTO = mmap; fork `src/llama-mmap.cpp:716-750` (FADV_SEQUENTIAL, MAP_POPULATE, WILLNEED) | load mode 1 = mmap, same code; lazy-host skip off (`engine/plan/src/load_prefetch.rs`) | yes |
| repack (`use_extra_bufts`) | true | false (disk stream); IQ4_XS has no repack type (run 59) | yes in effect |
| experts in RAM | `llm_ffn_block_regex` for 41 blocks (`:1284-1288`) | the same (`moe_override.rs`) | yes |
| op offload threshold | 32 (ggml default); only the used experts are copied (fork `src/llama-context.cpp:3155-3237`) | the same library | yes |
| random tokens | `std::rand()` unseeded | C `rand()` in the same order (`core.rs:298`); no other `rand()` caller in the fork (grep) | yes |
| expert prefetch / MoE cache | none | `CLUAIZ_EXPERT_PREFETCH=off`, none | yes |

Ranked before the test: (1) n_batch, the only difference that is inactive at pp512 and active at pp2048;
(2) the page cache state left by the load warmup; (3) n_ctx; (4) the thread pool (CPU only, cannot explain bytes).

## 2. Setup (run 63's, except as noted)

| Item | Value |
|---|---|
| Machine | GCP `g2-standard-8` (<vps>): L4 23 GB, 8 vCPU, 31 GB RAM, Debian 13, kernel 6.12.117, driver 620.70.06 |
| VRAM | `vram_hog.py 0 8`: 14,424 MiB used before the runs (runs 59 and 63: 14,424) |
| RAM | each run in its own `systemd-run --scope -p MemoryMax=12288M -p MemorySwapMax=0`, page cache dropped before each run |
| Model, placement | Qwen3.6-35B-A3B UD-IQ4_XS; llama-bench `-ngl 99 -ncmoe 41 -ctk f16 -ctv f16 -fa auto -t 4`; `cluaiz bench --n-gpu-layers all --n-cpu-moe 41 -c 16384 --cache-type-k f16 --cache-type-v f16 --flash-attn auto -t 4`, `CLUAIZ_EXPERT_PREFETCH=off`, `CLUAIZ_MTP=off` |
| Tests | `-p 512,2048 -r 2 -n 0`, one load per run, 3 rounds, order rotated (round 1 starts with C, round 2 with LF, round 3 with C-b2048) |
| llama-bench | the fork build of runs 48, 59, 63 (`8676a916e`) |
| Cluaiz | release build on the VM (`buildcause.sh`) of Engine `961a673f` as it was before the history rewrite (Engine code equal to run 63's `9dfd4f46`: `git diff 9dfd4f46 961a673f -- . ':!test'` was empty) with fork `96b0cfb2d`, plus `nowarm.patch`: `CLUAIZ_TEST_NO_WARMUP` set skips `NativeLlama::warmup`; unset, the code path is HEAD's. The patch is not in the Engine |
| Configs | `C`, `LF`; `C-b2048` (Cluaiz `-b 2048`, llama-bench's batch); `LFB512` (llama-bench `-b 512`, Cluaiz's batch); `C-c2048` (Cluaiz `-c 2048`); `C-nw` (Cluaiz without the load warmup). New in `pp_gap.sh` in this commit |

## 3. Results

Each cell is the tool's own mean of its 2 repetitions. Ratio = config / LF of the same round.

| Config | pp512 median (range) | pp2048 median (range) | pp2048 / LF per round | pp2048 / LF median | pp2048 phase MiB read, median |
|---|---|---|---|---|---|
| LF | 50.35 (50.28-67.53) | 101.08 (100.39-110.48) | 1, 1, 1 | 1.000 | 5,660 |
| C | 49.55 (49.45-49.86) | 92.45 (87.19-101.48) | 1.011, 0.789, 0.915 | 0.915 | 6,616 |
| C-b2048 | 48.94 (48.57-50.53) | 93.08 (87.46-107.26) | 1.068, 0.792, 0.921 | 0.921 | 6,380 |
| C-c2048 | 49.60 (49.32-50.42) | 96.71 (94.04-100.36) | 1.000, 0.875, 0.930 | 0.930 | 6,352 |
| C-nw | 50.60 (50.30-50.89) | 90.48 (89.46-93.46) | 0.901, 0.810, 0.925 | 0.901 | 6,622 |
| LFB512 | 50.57 (50.21-57.29) | 96.72 (91.30-102.28) | 1.019, 0.826, 0.957 | 0.957 | 6,320 |

Per-run rows, run totals and MiB per phase: `results/tables.md`, `results/phases.md`.

- pp512 is equal again: Cluaiz configs 48.57-50.89, llama-bench 50.21-50.35 apart from its usual one fast
  repetition (67.53 ± 22.14 in round 1, 57.29 ± 8.19 for LFB512 in round 3).
- pp2048: Cluaiz's default is again slower by median (0.915; run 63: 0.858), slower in 2 of 3 rounds.
- **Batch size is not the cause.** Cluaiz with `-b 2048` is the same as Cluaiz with 512 (93.08 against 92.45;
  per round 107.26 / 87.46 / 93.08 against 101.48 / 87.19 / 92.45). llama-bench with `-b 512` was 0.957 by
  median, within its own spread.
- **The load-time warmup is not the cause.** Without it Cluaiz has llama-bench's fault pattern (1.19-1.20 M
  major faults per run against 0.47-0.51 M, run time 548-551 s against 363-377 s) and is not faster at pp2048
  (90.48 median). Run 59 part e's one fast no-warmup run (106.84) did not repeat.
- **The 16384 context is not the cause.** `-c 2048` gave 96.71 by median (0.930), inside C's range.
- In every config the pp2048 speed follows the bytes read in that phase: Pearson r = -0.96 over all 18 runs
  (computed from `results/tables.md`). llama-bench at its defaults read 5,222-5,891 MiB there; every other
  config, including llama-bench at `-b 512`, read 5,806-7,022 MiB. Round 2 is the clearest: LF 110.48 at
  5,222 MiB, every Cluaiz config 87-97 at 6,352-7,022 MiB.

## 4. Root cause

Not found. Batch size, context size and the load warmup are rejected with 3 rounds each (section 3). Three
rejected candidates in a row: per the workspace rule the search stops here and the next candidates are proposed
instead of more guesses. No Engine default or bench parameter was changed, so no confirmation rounds were run.

What the data still allows, not tested:
- llama-bench builds a new context for each test (`llama-bench.cpp:2383`) and attaches its own thread pool with
  poll 50 (`:2397-2416`); Cluaiz keeps one context from load to the last test and attaches none. Test: Cluaiz
  with `-p 2048` alone against llama-bench `-p 2048` alone (removes the pp512 history from both), and a thread
  pool attached in `cluaiz bench` (test binary).
- The phase being compared starts with whatever the pp512 phase left in the page cache (11.7 GB of a 14.5 GB
  expert set). A tool that reads 0.6-1.4 GB more there may simply start from a worse cache state; the 18 runs
  show the state matters (r = -0.96) but not what sets it. A per-page `mincore` snapshot of the GGUF at the start
  of the pp2048 phase, both tools, would show it directly.
- llama-bench at `-b 512` read more (6,320 median) and was a little slower (0.957): weak evidence that the
  batch split matters for llama-bench but, as `C-b2048` shows, not for Cluaiz.

## 5. MTP off check (commit `d7ca5adf`, `CLUAIZ_MTP=off`)

`mtpcheck.sh`, after the A/B and with the VRAM hog stopped: Qwen3.5 2B MTP Q4_0, `cluaiz bench -p 0 -n 64 -r 1
-vv`, isolated home, `CLUAIZ_MTP=off` then `on` (`results/mtp/`).

| CLUAIZ_MTP | NextN layer loaded | MTP drafter | Plan line | tg64 t/s |
|---|---|---|---|---|
| off | no: llama.cpp logs `model has unused tensor blk.24.nextn.* -- ignoring` | none (no `Drafting` line) | `[MTP] Auto: on (measured faster on this placement)` | 162.71 |
| on | yes: `h_nextn buffer size = 4.01 MiB`, `set_embeddings_nextn` | `[MTP] Drafting up to 3 tokens per step with the model's 1 NextN layer(s)` | the same `Auto: on` line | 161.71 |

`off` really turns MTP off: the load leaves the NextN tensors out and no drafter is made
(`engine/llama/src/native/core.rs:655-657`, the override wins over the plan). The log is misleading, though:
the plan's `[MTP] Auto: ...` line (`engine/runtime/src/native_load.rs:224`) is printed before the override and says
`on` in both cases. `cluaiz bench` does not draft (its decode loop has no MTP), so the t/s column does not
measure MTP.

## 6. Problems in this run

- The first start (13:41 UTC, after `waitcause.sh` saw 10 quiet minutes) copied only the `cluaiz` binary out of
  `target/release`; it looks for its `.so` files next to itself (RUNPATH `$ORIGIN`), so every Cluaiz config
  ended at once (`libmtmd.so.1: cannot open shared object file`). A first stop missed the harness, and two
  instances overlapped for a few minutes. Both attempts were stopped and left on the VM in
  `~/ppcause/failed-start-1` and `failed-start-2`; none of their numbers is used. The counted run started at
  13:59:27 UTC with the binary and its libraries in `~/ppcause/bin` (md5 equal to `target/release/cluaiz`, `ldd`
  complete) and the VRAM hog alone on the GPU (14,424 MiB). `buildcause.sh` here is the fixed version.

## What this does not prove

- 3 rounds per config on one VM; the spread within one config (C 87-101) is as large as the gap, so a
  difference smaller than about 10% between two Cluaiz configs is not resolved.
- Anything outside this shape (experts larger than the 12 GB RAM cap, read from the SSD).
- The thread pool, the per-test context and the page cache state at the phase start (section 4).

## Pending

| Item | Why |
|---|---|
| Cause of the pp2048 gap (0.915x here, 0.858x in run 63) | batch, context and load warmup rejected; next: `-p 2048` alone on both tools, a thread pool in `cluaiz bench`, a `mincore` snapshot at the pp2048 phase start |
| The `[MTP] Auto: on` line under `CLUAIZ_MTP=off` | the override works, but the log says `on`; whether to print the override is the owner's call |
