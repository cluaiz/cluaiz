<!-- name: test/cloud/remote/ssd_prefetch
description: Measures the MoE expert prefetch and forced GPU cache on a model larger than RAM against official llama.cpp. -->

# MoE expert prefetch speed test (`test/cloud/remote/ssd_prefetch/`)

Measure whether the llama.cpp fork's MoE expert prefetch (`CLUAIZ_EXPERT_PREFETCH`, `engine/llama/README.md`)
together with the MoE GPU cache forced on (`CLUAIZ_MOE_CACHE=on`, `engine/plan/README.md`) makes generation
faster for a model larger than RAM, against official ggml-org llama.cpp on the same machine, file and
placement. Method: section 5 of `.agent/plans/engine/ssd-streaming/01-research-and-architecture.md`
(git-ignored plan folder).

## At a glance

- **Status:** `run_prefetch.sh` has not been run on its real model. `run_prefetch_laptop.ps1` ran in run 44
  (and the lazy-load configs in run 61); `run_case_g.sh` ran in run 48 (tables by `summarize_fair.py`);
  `pp_gap.sh` ran in run 75. Results and the prefetch's Auto rule (`engine/plan/src/expert_prefetch.rs`: on
  only for a MoE cache of at least 20% of the experts on Linux): `test/benchmarks/ssd-streaming-moe.md`.
- **Platform:** `run_prefetch.sh`: the L4 VM of runs 13 and 48 (GCP `g2-standard-8`: L4 22.5 GB,
  4 cores / 8 threads, 31 GB RAM), Ubuntu or Debian with CUDA, Linux only (`/proc/diskstats`,
  `drop_caches`). `run_prefetch_laptop.ps1`: the RTX 3050 Windows laptop (see its section below).

## Flow

```mermaid
graph LR
    A["build official llama-bench + llama-server<br/>(skipped when present)"] --> B["gpt-oss-120b MXFP4<br/>(cluaiz pull when missing)"]
    B --> C["two Cluaiz plans: default and CLUAIZ_MOE_CACHE=on<br/>(ngl, n_cpu_moe, KV, cache MiB, CPU share)"]
    C --> D["3 rounds x 6 configs, order rotated each round"]
    D --> E["per run: drop page cache, sampler + iostat,<br/>/usr/bin/time -v, raw .md/.log kept"]
    E --> F["results.md (per run + median), placements.txt,<br/>prefetch_lines.txt"]
```

## Configurations

| Config | Tool | Placement and flags |
|---|---|---|
| `L-mmap` | llama-bench | the Cluaiz default plan's `-ngl` / `-ncmoe` / KV type, plain mmap |
| `L-dio` | llama-bench | the same with `-lm dio` (skipped if this llama-bench has no `dio` load mode) |
| `L-cache` | llama-bench, or llama-server if llama-bench has no `--moe-cache-mib` | the forced-cache plan's `-ngl` / `-ncmoe`, `--moe-cache-mib` and `--moe-cache-cpu-share` of that plan |
| `C-plan` | `cluaiz bench` | default plan (a model read from disk gets no MoE cache, so the prefetch never runs) |
| `C-cache` | `cluaiz bench` | `CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=off` |
| `C-pf` | `cluaiz bench` | `CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=on` |

Every run: page cache dropped first, `-t 4 -p 512 -n 128,400 -r 1` (`REPS` changes `-r`), flash attention
auto. Pairs to compare: `C-plan` with `L-mmap`, `C-cache` with `L-cache`, `C-pf` with the best of
`L-mmap` / `L-cache` / `C-plan`.

Recorded per run: prompt and generation t/s (each tool's own table), peak RSS (`/usr/bin/time -v`), peak
VRAM and lowest free RAM (sampler, 1 s), page cache size, disk read MB/s from `/proc/diskstats` of the
partition holding the model (mean over the run), `iostat -x` (queue depth), the placement and MoE cache
size each Cluaiz run really used (`placements.txt`) and the prefetch counters (`prefetch_lines.txt`).

## Commands

On the dev machine (Git Bash, Engine root), with the local commits that hold the switch and the prefetch
(fork `a2d0fbed9` with Engine `f25d2f6f`, the switch `47d5d9aa`, the log filter `27654fce` that lets the
plan parser read llama.cpp's `MoE cache size` line). They are not pushed, so the VM gets a source tarball:

```bash
tar -czf /tmp/cluaiz-src.tgz --exclude=./target --exclude=.git .
test/cloud/vm.sh create l4-prefetch g2-standard-8 nvidia-l4
test/cloud/vm.sh put l4-prefetch /tmp/cluaiz-src.tgz cluaiz-src.tgz
test/cloud/vm.sh put l4-prefetch test/cloud/remote/linux_build.sh linux_build.sh
test/cloud/vm.sh put l4-prefetch test/cloud/remote/ssd_prefetch/run_prefetch.sh run_prefetch.sh
test/cloud/vm.sh ssh l4-prefetch "bash linux_build.sh ~/cluaiz-src.tgz 89"     # release build, CUDA for the L4
test/cloud/vm.sh ssh l4-prefetch "CLUAIZ=~/cluaiz/target/release/cluaiz nohup bash run_prefetch.sh ~/results-ssd-prefetch > run.log 2>&1 &"
```

Back on the dev machine: `test/cloud/vm.sh get l4-prefetch results-ssd-prefetch test/cloud/runs/<date>-runN-ssd-prefetch/`,
write the results into `test/benchmarks/ssd-streaming-moe.md` from the raw files (skill `test-records`), `test/cloud/vm.sh delete l4-prefetch`.
The `vm.sh create` arguments follow `vm.sh`'s usage line (`<name> <machine> <gpu-type|none>`); the GPU type
`nvidia-l4` is not written in any earlier run record, so check it against the GCP accelerator list first.

## Proposed pass bar (the owner decides the final numbers)

From section 5.5 of the plan:

| Check | Pass |
|---|---|
| `C-plan` against `L-mmap`, `C-cache` against `L-cache` | generation within ±3% (median of 3) |
| `C-pf` against the best of `L-mmap`, `L-cache`, `C-plan` | generation >= 1.2x, median of 3, the ranges of the rounds do not overlap |
| Mechanism | disk MB/s and queue depth higher with `C-pf` than `C-cache`, disk bytes per token not higher |
| Safety | no crash, no OOM, peak RSS within the plan's RAM budget |

A result that does not pass is recorded as such; Auto stays off and nothing public changes.

## Decisions
| Decision | Why | Alternative not taken |
|---|---|---|
| llama-bench gets the placement Cluaiz planned | the comparison is the same placement, as in run 14 (`ssd_same_settings.sh`) | llama.cpp's `--fit` (a different placement; measured in run 13) |
| One run per config per round, page cache dropped before each, order rotated | section 5.3: a warm page cache or a fixed order would favour whichever runs later | `-r 3` in one process (the 2nd and 3rd repetitions read from a warm cache) |
| llama-server fallback for `L-cache` | the pinned fork's `tools/llama-bench/llama-bench.cpp` has no `--moe-cache-mib` (grep, 2026-11-22); llama-server has it (run 13 used it). Its numbers come from one request, not llama-bench's method, so they are marked `llama-server` in the table | leave the cache baseline out |
| Each Cluaiz run plans again; `placements.txt` keeps what it used | a manual placement in Cluaiz turns the MoE cache off (planner rule), so the cache runs must use the planner | pinning `--n-cpu-moe` (no cache then) |
| Resumable (`<tag>.done`) | Spot VMs stop; finished runs are kept | one uninterrupted loop |

## What was checked without a run

- `bash -n run_prefetch.sh`: no syntax error.
- The plan parser on a real `cluaiz serve -vv` log of OLMoE (2026-11-22, laptop) printed
  `-1 16 4096 f16 1642 0.8` (ngl, n_cpu_moe, n_ctx, KV, cache MiB, CPU share), matching that log.
- The summary step on hand-made tables in the llama-bench, `cluaiz bench` and llama-server formats (fake
  numbers, format only): each row and the medians were filled.

Not checked: the build, the pull, `drop_caches`, the sampler, iostat, llama-server's flags on the official
build, `-lm dio` on the official build, the time the whole run takes (expected to fit in `vm.sh`'s 3 h
`MAX_RUN`; not measured).

## Windows laptop script (`run_prefetch_laptop.ps1`): the laptop MoE test with the prefetch

Repeats the alternated comparison of `test/cloud/runs/2026-11-16-run20-laptop-moe-disk/` (Gemma 4 26B A4B
IQ4_XS, 14,205,004,928 bytes, official llama.cpp `b12428`, 4 GB laptop GPU) and adds the MoE cache and the
prefetch. PowerShell 5.1, no extra tools.

| Config | Command |
|---|---|
| `L` | `llama-bench -m <gguf> -p 512 -n 128 -r 3 -t 8 -ngl 99 -ncmoe 30 -fa 1` (the baseline command; KV f16 is llama-bench's default) |
| `C` | `cluaiz bench <gguf> -p 512 -n 128 -r 3 -t 8 -c 1024 --flash-attn on --cache-type-k f16 --cache-type-v f16 --n-gpu-layers all --n-cpu-moe 30`, `CLUAIZ_EXPERT_PREFETCH=off` (the baseline command) |
| `C-plan` | the same without `--n-gpu-layers` / `--n-cpu-moe` (the planner's placement), `-vv` |
| `C-cache` | as `C-plan`, `CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=off` |
| `C-pf` | as `C-plan`, `CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=on` |

Why the cache rows cannot use `--n-cpu-moe 30`: a placement given by the user is the planner's
"User setting" candidate, which never gets a MoE cache (`engine/plan/src/planner.rs`), and the prefetch
runs only in the cache's upload path. Checked on OLMoE with `CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=on
--n-gpu-layers all --n-cpu-moe 16 -vv` (2026-11-23): the plan was `User setting: planner bypassed`, with no
MoE cache line and no prefetch line. So `C-cache` and `C-pf` use the planner's placement, and `C-plan` is
their baseline with the same placement. Which placement the planner picks for Gemma on this laptop is
not known before the run; the `placement` column of `results.md` shows it for every run.

`-vv` on the planner rows: llama.cpp's info lines (the `MoE cache size` line and the fork's
`llama_cluaiz_moe_prefetch` counters) reach the log only at debug level. `C` keeps the baseline command
without `-v`.

Before round 1, a warm-up load with the baseline placement (`-p 0 -n 8 -r 1`) lets the new isolated home
learn the VRAM the model takes, as the first forced load did in the first laptop run (without it the planner keeps its
fixed 1 GB VRAM gap and chose CPU only there). Then 3 rounds, the order rotated each round; the file
cache is not emptied (that needs a tool such as RAMMap), so rotation is the control, as in the first laptop run.

Per run, in the output folder: `<tag>.md` (stdout, the speed table), `<tag>.log` (stderr), `<tag>.engine.log`
(the part of `cluaiz_Core.log` the run wrote), `<tag>.perf.csv` (`typeperf` every second: free RAM, disk
read bytes/s and read queue of `_Total`, GPU dedicated memory), `<tag>.info.txt` (free RAM before and
after, exit code, seconds). `results.md` has one row per run, the median per config, and the baseline's
numbers (llama.cpp 15.9, Cluaiz 14.9 t/s generation) beside them. It warns when a `C-cache` / `C-pf` run
shows no MoE cache or a `C-pf` run no prefetch line. The user and machine names in the raw files are
replaced by `<user>` and `<machine>` at the end.

The model must be complete: the script stops before any run when the file size is not
14,205,004,928 bytes (`-ExpectedSize` changes it).

```powershell
# Engine root, after `cargo build -p cmd` (the script uses target\debug\cluaiz.exe; -Cluaiz changes it)
powershell -ExecutionPolicy Bypass -File test\cloud\remote\ssd_prefetch\run_prefetch_laptop.ps1
```

Output goes to `%LOCALAPPDATA%\cluaiz-test\prefetch-laptop-<time>\` (`-Out` changes it), the isolated home
to `%LOCALAPPDATA%\cluaiz-test\prefetch-home-<same name>\` (`-CluaizHome`). No server is started, so no
port is used. Copy the output folder to `test/cloud/runs/<date>-runN-*/results/` and write the results into
`test/benchmarks/ssd-streaming-moe.md`.

Checked (2026-11-23, format only, not speed numbers): with OLMoE-1B-7B Q4_K_M (`-Rounds 1 -Reps 1
-Prompt 64 -Gen 16 -NCpuMoe 16`) every config ran with exit 0, every file above was written, and
`results.md` filled every row and median; `C-pf` showed `191 calls, 2245 MiB` prefetched. A wrong file size
stopped the script with no output folder.

Not checked: the Gemma run itself and its total time; whether the planner gives Gemma a MoE split with a
cache on this laptop; the release build (the first laptop run used `target\release`; this script defaults to the debug
build `cargo build -p cmd` makes, whose llama.cpp is built with CMake's Release flags, while the Rust code
is not optimized); cold-cache runs.

`-Set lazy` (run 61, `test/cloud/runs/2026-11-24-run61-laptop-lazy-load/`): the load-prefetch skip
(`CLUAIZ_LAZY_HOST_WEIGHTS`) and the Windows ask-once expert prefetch. Configs `L`, `C` / `C-lazy` (the baseline's
placement without / with the skip), `C-plan` / `C-plan-lazy` (the planner's placement), `C-cache` / `C-pf` /
`C-lazy-pf` (`CLUAIZ_MOE_CACHE=on`, prefetch off / on / on plus the skip); every Cluaiz config runs with `-vv`
so the log has the load timestamps. `-Only C,C-lazy` keeps only the named configs (one string, split on
commas; the parameter is not `-Configs` because PowerShell names are case-blind and the script has a
`$configs` table). `analyze_lazy_load.py <out folder>` writes `lazy_load.md`: per run the free RAM before and
the lowest during the run, load seconds (the Engine's `MEMORY TRACE` lines before and after the model load),
disk GB read in that window and in the whole run (typeperf), pp/tg, placement, MoE cache, skipped MiB and the
prefetch counters; then the median per config. With 0.2-1 GB free RAM a pp512 pass took 11 minutes, so run 61
used `-Reps 1`.

## `pp_gap.sh` (the prompt-speed gap, run 75)

Run 48's bench shape (L4, ~8 GB VRAM by `vram_hog.py`, 12 GB cgroup, `--n-cpu-moe 41`, no MoE cache), with a
sampler that also records the cgroup's `memory.stat` and the process's RSS and major faults, and time-stamped
logs (`llama-bench --progress`, `cluaiz bench`'s "running" lines). Configs (`CONFIGS`): `LF` fork llama-bench,
`C` `cluaiz bench`, `CB` `cluaiz bench` with the `CLUAIZ_BASE` binary (before / after in one session), `CN` `C`
with `CLUAIZ_ENV` set, `C-c<N>` `C` with `-c N`, `LF-b<MiB>` `LF` beside `balloon.py` holding that much anonymous
memory in the same cgroup, `C-b<N>` `C` with `-b N` and `LFB<N>` `LF` with `-b N` (llama-bench's default batch
is 2048, Cluaiz's 512; the micro-batch is 512 on both), `C-nw` `C` with `CLUAIZ_TEST_NO_WARMUP=1` (skips the
load-time warmup only in a test binary built with that switch, see the pp2048 cause run). Usage on the VM:
`CLUAIZ=<binary> CLUAIZ_BASE=<binary> PP=512,2048 TG=128 REPS=2 ROUNDS=2 CONFIGS="C CB LF" bash pp_gap.sh <out>`,
then `python3 analyze_pp_gap.py <out>` for seconds, MiB read, faults and memory per phase. Results:
`test/benchmarks/ssd-streaming-moe.md` (run 75). Use `--mtp off` where MTP must stay off.

## Run 48 additions to `run_case_g.sh` (fair baselines and the prompt-speed gap)

Fair-comparison configurations, one load on each side (`CONFIGS` picks them):

| Config | Tool and flags | Why |
|---|---|---|
| `L-serve` | `llama-server -ngl 99 -ncmoe 41 -c 16384 -t 4 -fa auto`, one load | llama.cpp without its MoE cache, on the same client as the rows below |
| `L-serve-mc` | the same with `--moe-cache-mib 3600 --moe-cache-cpu-share 0.9` (`MC_MIB`, `MC_SHARE`) | llama.cpp with its own upstream MoE cache, the size the Cluaiz planner picks here (3602.69 MiB, 0.9) |
| `C-serve` | one `cluaiz -vv serve` process (port 8011), prefetch off, planner default | one load for every request, like llama-server |
| `C-pf-serve` | the same with `CLUAIZ_EXPERT_PREFETCH=on` | the prefetch on one load |
| `B-serve` | `bmoe-server --auto --threads 4 -c 16384` (BigMoeOnEdge; `BMOE_ARGS` replaces `--auto`) | the closest competitor on SSD streaming, with its own planner |
| `B-cli` | `bmoe-cli --auto -n 128` on a short prompt | BigMoeOnEdge's own generation figure (it has no llama-bench style command) |
| `C-off` | `cluaiz bench`, `CLUAIZ_MOE_CACHE=off` | whether the MoE cache causes the prompt gap |
| `C-b2048` | `cluaiz bench --batch-size 2048 --ubatch-size 512` | llama-bench's batch sizes; Cluaiz's default is 512 / 512 (`engine/core/src/settings.rs`) |
| `C-pin-c4608` | `C-pin` with `-c 4608` | whether Cluaiz's 16384-token context slows the prompt (llama-bench sizes its context to each test) |
| `LF-bench` | llama-bench built from the Cluaiz fork (`FORK_BENCH`, default `~/fork-bench-build/bin/llama-bench`), same command as `L-bench` | whether the llama.cpp version causes the prompt gap |
| `C-auto` | `cluaiz bench` with no `CLUAIZ_EXPERT_PREFETCH` | today's default: the planner's Auto rule decides the prefetch |
| `B-serve-pin`, `B-serve-pin0` | bmoe-server `--no-auto --moe-stream --gpu-layers 99 --device-cache-mb 3600`, with and without `--device-cache-cpu-share 0.9`, f16 KV | BigMoeOnEdge on Cluaiz's placement. In run 48 neither loaded the model: the first is refused while streaming, the second puts the experts on the GPU too (out of memory) |
| `L-bench-mc` | llama-bench with the cache flags | **not usable**: llama-bench has no `--moe-cache-mib`, neither in `b54752d` (`llama-bench --help`) nor in upstream master `a8f8294` (2026-11-23). llama.cpp's cache is measured through `L-serve-mc` instead |

The serve rows use `fair_client.py`: case G's four prompts (greedy, thinking off, the same body for every
server), then three long prompts for prompt speed (`TEXT_PP`, default `512,2048,4096` approximate tokens;
the real counts are in each answer's `prompt_n`). llama-server and bmoe-server return llama-server's
`timings`; `cluaiz serve` does not, so its speeds come from the `llama.cpp timings:` lines of its log
(`<tag>.homelog.txt`). The serve rows wait for an HTTP 200 on `/health` (llama-server answers 503 while it loads).

## Failure and recovery

- No plan read, or the forced plan has no cache (less than 256 MiB of VRAM left): the script stops with a
  message pointing to `plans.log`.
- A Spot stop: start the VM and run the same command; finished runs are skipped.
- A run that fails: its exit code is in `<tag>.time`, its stderr in `<tag>.log`; the summary leaves its cells
  empty.

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| `run_prefetch.sh` on gpt-oss-120b | no number exists yet | the owner's yes and the VM cost |
| Windows laptop run with a MoE cache for the ask-once prefetch | run 61's 9 cache runs got no cache (usable VRAM 2.15 GB) | about 2.7 GB of free VRAM |
| Windows laptop run with a big MoE cache | run 44 had at most 452 MiB of cache on the 4 GB GPU, so Windows has no measurement of the shape where Auto turns the prefetch on (Auto stays off on Windows) | a GPU with more free VRAM, or the 4 GB GPU with nothing else on it |
| Final pass numbers | the bar above is proposed | the owner |
| `L-cache` / `L-bench-mc` through llama-bench | a different method from the other rows when llama-server is used | a llama-bench with `--moe-cache-mib`; upstream master `a8f8294` (2026-11-23) has none |
