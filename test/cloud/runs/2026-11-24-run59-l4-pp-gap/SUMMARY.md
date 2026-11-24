# Run 59: the prompt-speed gap between `cluaiz bench` and llama-bench on run 48's shape

Date: 2026-11-24 (02:11 to 06:04 UTC). Raw files: `results/part-a` to `results/part-e` (one folder per batch of runs,
2.0 MB in all, no file over 1 MB; user name replaced by `<user>`). Script: `test/cloud/remote/ssd_prefetch/pp_gap.sh`,
phase tables by `analyze_pp_gap.py <part>`. On the VM the folders were named `results-run55a` to `results-run55d` and
`results-run59e` (run 55 was taken by another run while this one ran).

## Question

Run 48: at the same placement with no MoE cache, `cluaiz bench` ran at 0.90x of the fork's llama-bench at pp512 and
0.79x at pp2048. The cache, the llama.cpp version, the batch size and the threads were already ruled out. Why?

## Setup (every number below unless a row says otherwise)

| Item | Value |
|---|---|
| Machine | GCP `g2-standard-8` standard VM (<vps>): NVIDIA L4 23 GB, 8 vCPU, 31 GB RAM, NVMe boot disk (`read_ahead_kb` 128; reads peak at about 186 MiB/s), Debian 13, kernel 6.12.117 (6.12.118 again booted without the NVIDIA module, so `grub-reboot` into 6.12.117), driver 620.70.06 |
| VRAM | `vram_hog.py 0 8`: 14,424 MiB used before each run, about 8 GB left |
| RAM | each run in its own `systemd-run --scope -p MemoryMax=12288M -p MemorySwapMax=0` cgroup, page cache dropped before each run |
| Model | Qwen3.6-35B-A3B UD-IQ4_XS (run 48's file, experts 14.5 GB) |
| Placement | llama-bench `-ngl 99 -ncmoe 41 -ctk f16 -ctv f16 -fa auto -t 4`; `cluaiz bench --n-gpu-layers all --n-cpu-moe 41 -c 16384 --cache-type-k f16 --cache-type-v f16 --flash-attn auto -t 4`, `CLUAIZ_EXPERT_PREFETCH=off`; no MoE cache on either side |
| Tests | `-p 512,2048 -r 2` (part b also a second `2048`), `-n 0`; part e `-n 128` |
| llama-bench | the fork build of run 48 (`8676a916e`) |
| Cluaiz | release build of Engine `03de2c96` (run 48's). Test binaries, each built on the VM from `03de2c96` with one local change and deleted afterwards: `cluaiz-nowarm` (env `CLUAIZ_TEST_NO_WARMUP` skips the load-time warmup), `cluaiz-rand` (env `CLUAIZ_TEST_GLIBC_RAND` draws the bench tokens with glibc `rand()`), `cluaiz-fix` (`03de2c96` + the bench change of commit `370cf8ac` + the no-warmup switch, which is off unless its env is set) |
| Sampler | every second: cgroup `memory.stat` (anon, file, file_mapped), the process's RssAnon / RssFile / threads / major faults, disk sectors read |

## 1. Field by field: what differs between the two tools (llama.cpp's own log lines, parts a and b)

| Field | llama-bench | `cluaiz bench` |
|---|---|---|
| load mode | `LLAMA_LOAD_MODE_AUTO`, which is mmap when every device supports it (fork `src/llama-model.cpp:1703`) | 1 = mmap |
| lazy mode, `no_host`, overrides | AUTO, false, `llm_ffn_block_regex(i, LLM_FFN_EXPS_REGEX)` to the CPU buffer type | the same (`engine/llama/src/moe_override.rs`) |
| `use_extra_bufts` (repack) | true | false (disk stream); the IQ3_S / IQ4_XS experts have no repack type, and both logs show the experts moved from `CUDA_Host` to `CPU` |
| n_ctx | n_prompt + n_gen per test (512, 2048) | 16384 |
| n_batch / n_ubatch | 2048 / 512 | 512 / 512 (run 48: 2048 made no difference) |
| flash attention, KV type, op_offload, n_seq_max, swa_full | auto, f16, 1, 1, false | the same |
| warmup | one prompt run per test | one full-batch decode at load (`NativeLlama::warmup`) plus one prompt run per test |
| random tokens | `std::rand() % n_vocab`, unseeded (fork `tools/llama-bench/llama-bench.cpp:2184`) | its own xorshift64 stream (`engine/llama/src/native/core.rs` `bench`, before `370cf8ac`) |

Both tools left the same memory to the page cache: during the prompt tests, cgroup anon was 410-440 MiB and file
11.70-11.76 GB for each (analyzer table, part a).

## 2. Every pp phase reads from the disk (part a)

Phase table of `r1_LF` and `r1_C` (part a):

| Phase | seconds | MiB read | process major faults |
|---|---|---|---|
| llama-bench load | 100 | 18,218 | 116 |
| llama-bench pp512 warmup (first prompt after load) | 358 | 9,963 | 1,158,944 |
| llama-bench pp512 run 1 / run 2 | 9 / 9 | 1,498 / 1,182 | 5,357 / 7,526 |
| llama-bench pp2048 warmup + 2 runs | 61 | 5,707 | 24,888 |
| Cluaiz load (with its warmup) | 112 | 19,999 | 2,638 |
| Cluaiz pp512 warmup + 2 runs | 194 | 13,823 | 461,264 |
| Cluaiz pp2048 warmup + 2 runs | 69 | 6,389 | 37,787 |

Prompt speed follows the bytes read. On pp2048, Cluaiz read 6.3-6.4 GB per test (parts a, c and d) and
llama-bench 5.5-5.7 GB. The first prompt after load also takes minutes in both tools (llama-bench: 355-361 s for
512 tokens in every part), mostly spent on 4 KiB page faults.

## 3. The tests that found the causes

**Context size (part b).** With `-c 2048` Cluaiz was slower (pp512 40.16, pp2048 78.13 / 74.91) and read 58 GB
instead of 49 GB. Its log shows `[MTP] Auto: on with the per-answer guard (no measurement for this placement)` and
an MTP context (KV 4 MiB, 86 nodes) beside the main one. The harness sets `CLUAIZ_MTP=off`, but that value is never
read: `native::speculative::requested` accepts only on / 1 / true. The `-c 16384` runs logged
`Auto: off (measured slower on this placement)`. So the `-c 4608` slowdown in run 48 (pp512 38.49, 66 GB read) came
from MTP, not from the context size. Not a cause of the gap.

**Load-time warmup (part c, 2 rounds; binary `cluaiz-nowarm` in both rows):**

| Config | pp512 | pp2048 | major faults per run | first test (pp512) phase |
|---|---|---|---|---|
| warmup on (default) | 45.43 / 46.54 | 90.46 / 96.90 | 504,590 / 499,202 | 195 / 195 s |
| warmup skipped | 47.69 / 48.73 | 95.69 / 91.21 | 1,215,047 / 1,222,019 | 385 / 386 s |

With the warmup skipped, Cluaiz shows llama-bench's fault pattern and llama-bench's slow first prompt. The warmup
explains part of the pp512 gap but not pp2048, and it halves the time of the first prompt after load.

**Random tokens (part d, 2 rounds; binary `cluaiz-rand` in both Cluaiz rows):**

| Config | pp512 | pp2048 |
|---|---|---|
| own xorshift stream (default) | 45.37 / 45.19 | 90.36 / 91.58 |
| glibc `rand()`, as llama-bench | 48.82 / 49.12 | 96.16 / 95.10 |
| llama-bench, same session | 65.18 (± 17.11) / 51.41 | 106.81 / 96.74 |

## 4. Root cause and fix

1. **Bench tokens** (`engine/llama/src/native/core.rs`, `NativeLlama::bench`, the xorshift closure before commit
   `370cf8ac`). When the model is read from the disk, the token stream decides which experts each batch reads, and
   each tool repeats its own stream in every run. Fixed in `370cf8ac`: tokens now come from the C library's
   `rand()` and are drawn in llama-bench's order (none for the BOS slot, no generation token in a prompt-only
   run). Only `cluaiz bench` changes; chat and serve never call this code. No other default changes.
2. **Load-time warmup** (`NativeLlama::warmup`). Kept: it costs 1-3 t/s at pp512 on this shape and saves about
   190 s on the first prompt after load.

## 5. Before and after in one session (part e, 2 rounds each, order rotated)

| Config | pp512 | pp2048 | tg128 | major faults per run | disk read per run |
|---|---|---|---|---|---|
| before: `03de2c96` | 45.24 / 45.33 | 90.13 / 91.69 | 11.81 / 11.35 | 513,564 / 514,781 | 41.6 / 41.4 GB |
| after: `03de2c96` + fix | 49.56 / 50.61 | 92.03 / 86.37 | 12.07 / 10.65 | 505,964 / 518,363 | 40.9 / 41.8 GB |
| after, warmup skipped (test only) | 51.09 / 50.77 | 106.84 / 91.81 | 12.74 / 12.51 | 1,185,222 / 1,203,890 | 38.3 / 39.3 GB |
| llama-bench (fork) | 50.76 / 51.13 | 94.83 / 99.59 | 11.79 / 13.04 | 1,216,645 / 1,205,329 | 39.2 / 38.3 GB |

- pp512: 0.89x of llama-bench before the fix, 0.98x after.
- pp2048: Cluaiz 86-92 and llama-bench 95-108 across runs 48 and 59. The spread inside each tool (up to ± 8) is as
  large as the gap, so a 5-10% pp2048 gap is neither proven nor ruled out.
- tg128: 10.65-12.07 against 11.79-13.04, the same within the spread (run 48: 10.82 against 11.14).

## 6. The planner trade-off (run 48 numbers, not changed here)

With `CLUAIZ_MOE_CACHE=off` the planner sets `n_cpu_moe = 32` instead of 41, which keeps 9 more expert layers on the
GPU. Result: pp512 112.86 against 44.33 (2.5x faster), but tg128 16.72 against 20.87 with the cache (0.80x) and
26.58 with the prefetch. Which side to favour on this shape is the owner's decision.

## Caveats

- One machine, one model and one shape (experts larger than the RAM cap, read from the SSD). When a model is held in
  RAM the token stream cannot change the bytes read, so the fix should not change speed there (not measured).
- 2 rounds per config. llama-bench's two pp512 runs often differ a lot (± 17 in one round), so its pp512 medians
  move between runs (50.76-65.18).
- The test switches (`CLUAIZ_TEST_NO_WARMUP`, `CLUAIZ_TEST_GLIBC_RAND`) existed only in the VM test binaries; they
  are not in the Engine.
- In parts c and d the analyzer's proc-majfault column is 0 for the test binaries (the sampler looked up the
  process name `cluaiz`); the per-run totals from `/proc/vmstat` in `steps.log` are complete.
- Disk and fault totals per run cover the whole VM.

## Pending

| Item | Why |
|---|---|
| pp2048 gap of 5-10%, if it is real | inside the run-to-run spread here; needs 4+ rounds per config |
| First prompt after load takes 3-6 minutes on this shape (both tools) | 4 KiB page faults from the disk while the page cache holds the file's tail after the load; not addressed here |
| `CLUAIZ_MTP=off` is not read | tests that want MTP off must pass `--mtp off`; whether the env should accept off is the owner's call |
| Planner choice: ncmoe 41 (cache, better generation) or ncmoe 32 (2.5x prompt) | owner decision, section 6 |
