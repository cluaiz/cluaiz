# Run 60: why the expert prefetch asked the OS for 173-215 GB on the Windows laptop, and the ask-once fix

Date: 2026-11-24. Machine: the run 20 / 44 laptop (Windows 11 build 26300, Ryzen 7 7435HS, 23.7 GB RAM,
RTX 3050 Laptop 4 GB). File: Gemma 4 26B A4B IQ4_XS (14,205,004,928 bytes), the run 44 model. The owner's apps
ran throughout; free RAM is given per part.

## Question

Run 44 (`../2026-11-23-run44-laptop-prefetch-low-ram/SUMMARY.md`): with the fork's MoE expert prefetch on, the
prefetch asked the OS for 173,143 and 215,027 MiB per run for 11,797 MiB of experts, and generation fell from
17.48 to 10.23 t/s, while round 1 read only 0.74 GB from disk. Where do the bytes come from, and what does
each asked byte cost?

## Root cause

1. Bytes: the fork asked for every fill and every CPU miss of every MoE cache upload, every time
   (`engine/llama/llama.cpp/src/llama-cluaiz-prefetch.h`, `llama_cluaiz_moe_prefetch`, fork commit
   `a2d0fbed9`; bytes = misses x expert size: 56,365 misses x 3.072 MiB = 173,163 MiB against 173,143 logged,
   `.agent/plans/engine/ssd-streaming/03-prefetch-auto.md` section 2). Nothing remembered what was asked
   before, so the same resident experts were asked again token after token. With a cache of 2.2-3.8% of the
   experts, 95% of the misses go to the CPU: about 15 MiB per call, 30 calls per token.
2. Cost: Windows `PrefetchVirtualMemory` does its work on the calling thread, also for pages already in memory,
   and saves nothing for them (part 1). The prefetch is called from the MoE cache's `copy()` on the compute
   thread, so that time adds straight to each token.

## Part 1: cost of one call (`pvm.rs`, `rustc -O`)

The program maps the file read-only (`CreateFileMappingW` / `MapViewOfFile`), picks N ranges of 1 MiB, 2 MiB
apart (one tensor's slice of N random experts is not contiguous), and times `PrefetchVirtualMemory` and a
touch of one byte per 4 KiB page. "Standby" = touched, then the process working set emptied
(`SetProcessWorkingSetSize(-1, -1)`), so the pages sit in the standby list. 50 iterations, median.
Free RAM: 3.1-3.9 GB. Raw output: `results/pvm_*.txt`.

| Pages | 15 x 1 MiB (run 44's 15 MiB per call) | 5 x 1 MiB |
|---|---|---|
| in the working set: `PrefetchVirtualMemory` | 648 and 658 us | 215 us |
| in the working set: touch | 42 and 46 us | 13 us |
| standby: `PrefetchVirtualMemory` | 1,235 and 1,395 us | 404 us |
| standby: touch after the prefetch / without it | 6,552 / 6,542 and 6,868 / 6,709 us | 2,176 / 2,086 us |
| not touched before (4 offsets): touch without prefetch | 36.2-38.8 ms | |
| not touched before (4 other offsets): prefetch, then touch | 12.1-13.7 ms, then 7.5-8.6 ms | |

Reading:
- Resident pages: about 43 us per MiB in the working set and 85 us per MiB on standby, and the touch after it
  costs the same as without it (standby pages still soft-fault). Pure cost.
- Pages read from disk: prefetch + touch took 20-22 ms against 36-39 ms for the touch alone (about 1.8x).
  This is the only case where the prefetch helps on Windows.
- Run 44 check: 15 MiB per call x 30 calls per token x 0.65-1.3 ms = 20-40 ms per token, the same order as the
  48 ms per token that run 44 lost.

## Part 2: replay of the call pattern, before and after the fix (`replay.rs`)

2,000 calls, each asking for 15 random 1 MiB slices out of an 11,797 MiB region of the file (the expert bytes
of this model), then touching them as the CPU matmul would. Modes: `every` = the fork before the fix (asks every
call), `once` = after the fix (skips slices asked before in this process), `none` = prefetch off. One process
per mode; free RAM 11.5-12.5 GB (the owner's apps had freed memory by then), so most of the region was
resident, as in run 44. Raw output: `results/replay_round0.txt` (first pass on a cold file),
`results/replay_rounds1-3.txt`.

| Round | every: asked MiB / prefetch s / total s | once: asked MiB / prefetch s / total s | none: total s |
|---|---|---|---|
| 0 (order none-every-once-once-every-none) | 30,000 / 3.08, 3.66 / 9.42, 10.82 | 10,833 / 1.49, 1.60 / 7.32, 7.58 | 11.82 (first, cold), 6.97 (last) |
| 1 | 30,000 / 5.87 / 13.36 | 10,816 / 3.05 / 10.55 | 6.90 |
| 2 | 30,000 / 5.09 / 11.04 | 10,845 / 1.61 / 7.87 | 11.74 |
| 3 | 30,000 / 25.13 / 33.76 | 10,877 / 9.56 / 16.91 | 7.96 |

Reading:
- Ask-once cuts the asked bytes by 64% (30,000 -> 10,816-10,877 MiB) and the time in `PrefetchVirtualMemory`
  by 48-68%, and the total by 21-50%, in every round.
- With most pages resident, no prefetch at all was still the fastest in 3 of 4 warm comparisons (6.90-7.96 s
  against 7.32-16.91 s for `once`); round 2's `none` (11.74 s) is the exception. On this laptop ask-once
  removes most of the cost, but the prefetch does not beat "off" when the experts are already in RAM.

## Part 3: live bench

The live A/B in run 61 (`../2026-11-24-run61-laptop-lazy-load/SUMMARY.md`, configs C-cache, C-pf, C-lazy-pf)
could not exercise the prefetch: with 1.0-2.2 GB of VRAM used by other programs the planner gave no MoE cache
in any of those 9 runs (no `MoE cache size` line), and without a cache the prefetch never runs. There is no
live before/after number for the fix on this laptop.

## Decision

- The fork asks each expert slice at most once per context on Windows (`ask_once` in
  `src/llama-cluaiz-prefetch.cpp`, `CLUAIZ_PATCHES.md` patch 1). Linux is unchanged (run 43 measured the
  every-time behaviour as 1.27-1.43x faster there).
- The planner's Auto rule (`engine/plan/src/expert_prefetch.rs`) is not changed: the fixed prefetch is not
  proven faster than prefetch off on Windows (part 2), and there is no live run with a cache (part 3). The
  prefetch stays off on Windows unless the setting or `CLUAIZ_EXPERT_PREFETCH=on` asks for it.

## Rerun

```
rustc -O pvm.rs -o pvm.exe
pvm.exe <model.gguf> 6000 15 1024 50          (offset MiB, ranges, KiB per range, iterations)
pvm.exe <model.gguf> 13000 15 1024 3 nopvm   (cold touch without the prefetch, an offset not read lately)
rustc -O replay.rs -o replay.exe
replay.exe <model.gguf> 1700 11797 2000 15 every|once|none <seed>
```

## Caveats

- The part 1 table is the first take (free RAM 3.1-3.9 GB), which went to the console only. The repeat with
  the same commands after run 61 (free RAM 13.3 GB) is the raw file: `results/pvm_resident_standby.txt`
  reproduces the resident and standby rows (646-668 us and 1,359-1,551 us per 15 MiB call, touches 41 us and
  7.4-7.5 ms). `results/pvm_cold.txt` could not reproduce the cold rows: after run 61 the file sat in the
  standby list (touches 7.3-16.7 ms, prefetch calls 1.8-3.1 ms), so the 36-39 ms against 20-22 ms comparison
  rests on the first take only.
- A shared laptop: disk and CPU were also used by the owner's programs (round 3's 25 s is that).
- The replay models the call pattern (uniform random experts); it is not a trace of real routing.

## Pending

| Item | Why |
|---|---|
| Live run with a MoE cache on this laptop, prefetch off against ask-once | needs about 2.7 GB of free VRAM (run 44's state); none of the 9 cache runs got a cache |
| A residency check before asking (Windows `QueryWorkingSetEx`) | would skip resident pages never asked; not measured, and standby pages show as not resident |
| Re-ask after eviction | a slice evicted after its one ask is read by page faults again (as with the prefetch off) |
