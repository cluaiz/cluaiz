# Run 48: fair baselines on the run 43 / 45 shape, and the search for the prompt-speed gap

Date: 2026-11-23 (13:28 to 22:49 UTC). Raw files: `results/` (`results.md` is every run and every median,
made by `test/cloud/remote/ssd_prefetch/summarize_fair.py results`; build and phase logs in `results/logs/`).

## Question

Runs 43 and 45 left two things open:

1. Run 43's "Cluaiz with the prefetch is 1.37-2.20x llama.cpp" was against llama.cpp **without** its own MoE
   cache, and Cluaiz loaded the model once per prompt while llama-server loaded it once. Is Cluaiz still faster
   against llama.cpp **with** its cache (`--moe-cache-mib`, upstream PR #29887), with one load on both sides, and
   against BigMoeOnEdge, the closest competitor on SSD streaming?
2. Run 45: Cluaiz's prompt processing was 9-11% behind llama-bench at pp512 / pp2048. Why?

## Setup (the same for every number below unless a row says otherwise)

| Item | Value |
|---|---|
| Machine | GCP `g2-standard-8` (<vps>): NVIDIA L4 23 GB, 8 vCPU (4 cores), 31 GB RAM, NVMe boot disk, Debian 13, kernel 6.12.117 (6.12.118 booted without the NVIDIA module; booted once into 6.12.117 with `grub-reboot`), driver 620.70.06 |
| VRAM | `vram_hog.py 0 8` held 13.90 GB, so about 8 GB was left (`results/hog.log`) |
| RAM | each run in its own `systemd-run --scope -p MemoryMax=12288M -p MemorySwapMax=0` cgroup (page cache counted); peak RSS of every run that loaded the model was 12,275-12,341 MiB |
| Page cache | dropped before every run |
| Model | Qwen3.6-35B-A3B UD-IQ4_XS, 18,209,036,576 bytes (experts 14.5 GB), the file of runs 23 / 43 / 45 |
| Threads | 4 everywhere (Cluaiz `n_threads = 4, n_threads_batch = 4` in its logs) |
| Rounds | 3, the order rotated each round (2 rounds for `C-pin-c4608`, 1 round for `C-auto`) |
| Cluaiz | release `cargo build -p cmd --release` (CUDA arch 89) of Engine `03de2c96` (contains the prefetch Auto rule `2419189e`) with fork `8676a916e` (the committed pointer `a2d0fbed9` plus one docs commit). Planner default plan: `n_gpu_layers = -1, n_cpu_moe = 41, n_ctx 16384, KV f16`, MoE cache 3602.69 MiB with 90% of the misses on the CPU (`results/plans.txt`) |
| llama.cpp | official `b54752d` (2026-11-15), `-ngl 99 -ncmoe 41 -fa auto -t 4`, f16 KV, plain mmap |
| BigMoeOnEdge | `Helldez/BigMoeOnEdge` `d02bb94` (2026-11-23), `bmoe-server` 0.48.0, llama.cpp submodule `09e3dc4`, built with `scripts/build-host.sh -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=89 -DBMOE_BUILD_DESKTOP=OFF -DBMOE_BUILD_TESTS=OFF` (no error; the second half of the build took 1,129 s, `results/logs/bmoe_build2.log`) |
| Script | `test/cloud/remote/ssd_prefetch/run_case_g.sh`; configurations in that folder's README ("Run 48 additions") |

Two methods, each with **one load per run**:

- **Serve** (`fair_client.py`): the server loads the model once, then gets run 43's four prompts (code, chat,
  list, long; greedy, thinking off, MTP off) and three long prompts for prompt speed (527 / 2165 / 4404 tokens
  in llama-server and bmoe-server, 525 / 2163 / 4402 in `cluaiz serve`). llama-server and bmoe-server report
  their own `timings`; Cluaiz's numbers are the `llama.cpp timings:` lines of `cluaiz serve -vv`.
- **Bench**: llama-bench or `cluaiz bench`, `-p 512,2048,4096 -n 128 -r 2`, one warm-up, random tokens.
  BigMoeOnEdge has no such command; `bmoe-cli --auto -n 128` on a short prompt is its own figure.

## 1. The fair comparison: generation, serve, one load each (t/s, median of 3, range in brackets)

| Config | MoE cache | Prefetch | code | chat | list | long | disk read per run |
|---|---|---|---|---|---|---|---|
| L-serve: llama-server | none | - | 6.17 (6.17-6.17) | 10.97 (10.94-11.16) | 8.73 (8.59-8.74) | 9.09 (8.99-9.12) | 91-126 GB |
| **L-serve-mc**: llama-server `--moe-cache-mib 3600 --moe-cache-cpu-share 0.9` | 3600 MiB, upstream | - | 7.63 (7.63-7.69) | 20.70 (20.02-20.81) | 13.15 (12.84-13.20) | 15.93 (15.57-16.11) | 45 GB |
| B-serve: bmoe-server `--auto` | its plan: 1856 MiB device cache, 8892 MiB RAM expert cache, O_DIRECT | its own, off by default | 4.51 (4.50-4.68) | 10.13 (10.09-10.13) | 6.29 (6.28-6.33) | 4.87 (4.82-4.97) | 75 GB |
| C-serve: `cluaiz serve`, `CLUAIZ_EXPERT_PREFETCH=off` | 3602.69 MiB (planner) | off | 7.74 (7.67-7.86) | 21.34 (21.13-21.76) | 13.44 (12.99-13.46) | 16.05 (15.71-16.38) | 47-48 GB |
| **C-pf-serve**: `cluaiz serve`, `CLUAIZ_EXPERT_PREFETCH=on` (= the default here, see below) | 3602.69 MiB (planner) | on | **10.21** (10.18-10.21) | **25.11** (24.91-25.24) | **18.83** (18.76-18.91) | **19.76** (19.63-19.88) | 44 GB |

| Ratio of medians | code | chat | list | long |
|---|---|---|---|---|
| C-pf-serve / L-serve-mc | 1.34x | 1.21x | 1.43x | 1.24x |
| C-serve / L-serve-mc (both the same upstream cache) | 1.01x | 1.03x | 1.02x | 1.01x |
| C-pf-serve / B-serve | 2.26x | 2.48x | 2.99x | 4.06x |
| L-serve-mc / L-serve (llama.cpp's cache on its own) | 1.24x | 1.89x | 1.51x | 1.75x |

The rounds of C-pf-serve and L-serve-mc do not overlap on any prompt.

**Is the prefetch on by default now?** Yes on this shape. `C-auto` (`cluaiz bench` with no
`CLUAIZ_EXPERT_PREFETCH`) logged `Expert prefetch : on, at most 3.52 GB per MoE cache upload` and
`Expert prefetch on: at most 3605 MiB per MoE cache upload` (`results/r1_C-auto.log`), as the Auto rule says
(Linux, cache 3.6 of 14.5 GB = 25%, at least 20%). So C-pf-serve is Cluaiz's default here, and C-serve is
Cluaiz with the prefetch turned off.

Evidence that llama-server's cache ran: the flags were accepted, generation rose 1.24-1.89x over L-serve, and
the disk read per run fell from 91-126 GB to 45 GB. llama-server prints no `MoE cache size` line at its
default log level, so there is no log line for it.

What this says:

- With the same upstream MoE cache on both sides, Cluaiz and llama.cpp generate at the same speed (1.01-1.03x).
  The gain over llama.cpp's best setting tested here is the prefetch alone: **1.21-1.43x**.
- Run 43's 1.37-2.20x was mostly llama.cpp's missing cache: the cache alone gives llama.cpp 1.24-1.89x.
- BigMoeOnEdge's `--auto` is the slowest of the three on this shape (4.5-10.1 t/s).

### Compared with run 43 (one load per prompt)

| Cluaiz with prefetch | code | chat | list | long |
|---|---|---|---|---|
| Run 43 C-pf-text, `cluaiz run` per prompt (4 loads) | 10.21 | 14.93 | 13.55 | 19.52 |
| Run 48 C-pf-serve, one `cluaiz serve` (1 load) | 10.21 | 25.11 | 18.83 | 19.76 |

The first prompt (code) is the same: it runs right after the load in both methods. Chat and list are faster
with one load, likely because experts read for earlier prompts are still in the cgroup's page cache (not
measured per prompt).

## 2. Generation, bench (tg128, t/s, median of 3)

| Config | Placement | MoE cache | tg128 |
|---|---|---|---|
| L-bench: llama-bench `b54752d` | `-ngl 99 -ncmoe 41` | none (llama-bench has no cache flag) | 10.49 (10.45-11.40) |
| LF-bench: llama-bench built from the Cluaiz fork `8676a916e` (same CMake flags as the official build) | `-ngl 99 -ncmoe 41` | none | 11.14 (10.49-11.43) |
| C-pin: `cluaiz bench --n-gpu-layers all --n-cpu-moe 41 -c 16384` | same as llama-bench | none (a user placement gets no cache) | 10.82 (10.53-10.88) |
| C-plan: planner default, prefetch off | ngl -1, ncmoe 41 | 3602.69 MiB | 20.87 (18.87-21.39) |
| C-pf: planner default, prefetch on | same | 3602.69 MiB | 26.58 (24.91-26.76) |
| C-auto: planner default, no prefetch env (1 round) | same | 3602.69 MiB | 26.68 |
| C-off: `CLUAIZ_MOE_CACHE=off` | **ngl -1, ncmoe 32** (the planner places differently without a cache) | none | 16.72 (16.68-16.91) |
| B-cli: `bmoe-cli --auto -n 128` | its plan: 11,400 MiB RAM expert cache | its own | 2.33 (2.33-2.33) |

At the same placement with no cache, Cluaiz and llama.cpp generate at the same speed (10.82 against 10.49 and 11.14).

`B-cli` reported `4476 experts dropped by the system under memory pressure` and `predicted 0.170 s/token,
measured 0.429 (-60%)` (`results/r1_B-cli.log`): its CLI's auto plan took an 11.4 GB RAM cache inside the
12 GB cgroup. That is BigMoeOnEdge's own default here, not a tuned setting.

## 3. The prompt-speed gap (part B)

Bench, prompt t/s, median of 3 (range in brackets):

| Config | What differs from L-bench | pp512 | pp2048 | pp4096 |
|---|---|---|---|---|
| L-bench (official `b54752d`) | - | 50.10 (49.16-50.37) | 108.12 (106.95-109.43) | 91.62 (86.74-96.28) |
| LF-bench (fork `8676a916e`) | llama.cpp version | 49.37 (48.73-49.92) | 108.14 (97.65-111.24) | 88.65 (84.61-101.34) |
| C-pin (no cache, same placement) | Cluaiz's bench and load path | 44.33 (43.72-46.12) | 85.91 (85.75-94.31) | 88.82 (82.65-92.59) |
| C-plan (planner, MoE cache) | + MoE cache | 44.33 (43.63-45.16) | 88.28 (86.91-91.24) | 91.09 (84.28-91.71) |
| C-b2048 (planner, `--batch-size 2048 --ubatch-size 512`; log `n_batch = 2048`, `n_ubatch = 512`) | + llama-bench's batch sizes | 43.91 (43.61-44.72) | 87.55 (86.49-87.59) | 91.33 (83.53-94.67) |
| C-pf (planner, prefetch on) | + prefetch | 44.50 (44.48-45.02) | 90.44 (88.85-96.79) | 88.97 (81.72-94.49) |
| C-pin-c4608 (2 rounds) | C-pin with `-c 4608` | 38.49 (37.95-39.03) | 78.03 (76.19-79.88) | 68.97 (66.58-71.36) |
| C-off (`CLUAIZ_MOE_CACHE=off`, ncmoe 32) | another placement | 112.86 (111.22-114.00) | 215.50 (210.40-217.57) | 245.45 (239.35-246.95) |

| Candidate cause | Test | Result |
|---|---|---|
| The MoE cache | C-pin (no cache) against C-plan (cache), same placement | **not the cause**: 44.33 / 85.91 / 88.82 against 44.33 / 88.28 / 91.09 |
| The llama.cpp version (fork against official) | LF-bench against L-bench, same command | **not the cause**: 49.37 / 108.14 / 88.65 against 50.10 / 108.12 / 91.62 |
| Batch size (Cluaiz 512 / 512, llama-bench 2048 / 512) | C-b2048 against C-plan | **not the cause**: 43.91 / 87.55 against 44.33 / 88.28 |
| Threads | logs | **not the cause**: 4 for generation and prompt on both |
| Context size (Cluaiz 16384, llama-bench sizes it to each test) | C-pin-c4608 against C-pin | **not the cause**: the smaller context was slower (38.49 / 78.03 / 68.97) and read more from disk (66 GB against 54-56 GB per run); why is not known (?) |

**Finding:** the gap is inside Cluaiz's own bench / load path (same llama.cpp library, same placement, same
batch, same threads, no cache): 0.90x of the fork's llama-bench at pp512, 0.79x at pp2048, equal at pp4096.
The exact cause is not known (?). Two differences were seen but not tested:

- major page faults per run: C-pin 571-623 thousand, llama-bench 1.26-1.28 million, for about the same bytes
  read (54-56 against 50-51 GB): the two read the file in pieces of different size (Cluaiz loads with
  `load_mode = 1`);
- the random tokens: `cluaiz bench` uses its own xorshift stream, llama-bench its own; on this shape the
  tokens decide which experts are read from the SSD.

The serve rows show the same direction with **identical** prompt text and the same upstream cache on both
sides: C-serve 30.13 / 101.71 / 150.91 against L-serve-mc 33.73 / 124.06 / 188.17 (0.89x / 0.82x / 0.80x).
So the token stream is not the whole answer either.

C-off is a separate fact: without a cache the planner keeps 9 more expert layers on the GPU (`n_cpu_moe =
32`), which gives 2.5x the prompt speed (112.86 against 44.33 at pp512) and 0.80x the generation (16.72
against 20.87; with the prefetch, which needs the cache, 26.58). On this shape the planner's cache placement
trades prompt speed for generation speed.

## BigMoeOnEdge on Cluaiz's placement: not run

- `B-serve-pin` (`--no-auto --moe-stream --gpu-layers 99 --device-cache-mb 3600 --device-cache-cpu-share 0.9
  --cache-type-k f16 --cache-type-v f16`): bmoe-server started but did not load the model: `"Device cache
  misses on the CPU" computes misses from the experts' resident copy: it needs "Expert streaming" off`
  (`results/r1_B-serve-pin.server.log`). Each round waited the 30 min limit (1,820 s).
- `B-serve-pin0` (the same without `--device-cache-cpu-share`): `allocating 16435.34 MiB on device 0:
  cudaMalloc failed: out of memory` (`results/r1_B-serve-pin0.server.log`): `--gpu-layers 99` also put the
  experts on the GPU. Stopped after this second failure. B-serve (its `--auto` plan) is the BigMoeOnEdge row.

## Caveats

- One machine, one model, one shape (model larger than the RAM cap, experts read from the SSD).
- Serve prompt speed and bench prompt speed are different methods (real text against random tokens, cold
  against after a warm-up); compare rows within one method only. llama-server without the cache was very
  slow on the long prompts (14-28 t/s) and read 91-126 GB per run; that row is shown as measured.
- BigMoeOnEdge ran with its own `--auto` plan (q8_0 KV, `n_ubatch 8192`, its own RAM cache with O_DIRECT, a
  1856 MiB device cache with no CPU share), not on the other rows' placement (see above). Its `--auto` measures
  the machine at each load (`plan: the storage stage took 32.9 s` and others) and kept its stored verdicts in
  its data folder across the rounds, as a user's install would.
- llama.cpp and Cluaiz used plain mmap; llama.cpp's `--load-mode none` was not tried.
- Text equality across tools was not checked.
- Disk and fault numbers are for the whole VM (`/proc/diskstats`, `/proc/vmstat`).
- No raw file is over 1 MB (`results/` is 4.6 MB in all), so all of them are in git. User name replaced by `<user>`.

## Pending

| Item | Why |
|---|---|
| Cause of the pp512 / pp2048 gap inside Cluaiz's bench path | narrowed (not the cache, version, batch, threads or context); next: compare the load mode / page-fault pattern, and run llama-bench with Cluaiz's token stream |
| Why a smaller context (`-c 4608`) read more from disk and was slower | not explained (?) |
| BigMoeOnEdge on the same placement | its flags refuse a device cache with CPU misses while streaming, and `--gpu-layers 99` puts the experts on the GPU; needs the right flag set from its docs |
| Planner choice between the cache placement (ncmoe 41, better generation) and ncmoe 32 (2.5x prompt) | a planner decision for the owner; this run shows both sides on one shape |
| `competitor-repos/.agent/compare/angles/ssd-streaming.md` | still says any "faster than" covers only llama.cpp without its cache; these numbers can replace that line |
