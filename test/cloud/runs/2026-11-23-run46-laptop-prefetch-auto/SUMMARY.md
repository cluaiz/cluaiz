# Run 46: laptop check of the expert prefetch's new Auto rule (run 44's shape could not be rebuilt)

Date: 2026-11-23. Same laptop as runs 42 and 44 (Windows 11, Ryzen 7 7435HS, 23.7 GB RAM, RTX 3050 Laptop
4 GB). Model: Gemma 4 26B A4B IQ4_XS, 14,205,004,928 bytes. Raw output: `results/`.

## Question

The planner's Auto rule for the expert prefetch (`engine/plan/src/expert_prefetch.rs`) is now: on only when
the MoE cache holds at least 20% of the experts and the OS is Linux. Does a real load on this laptop with
the setting left on Auto give no prefetch, and does `CLUAIZ_EXPERT_PREFETCH=on` still win?

## Setup

- Binary: debug `cargo build -p cmd` of the working tree on `00a20745` plus the Auto change. The final copy
  to `target/debug/cluaiz.exe` failed (`Access is denied`: another `cluaiz serve` ran from that file), so the
  runs used cargo's own output `target/debug/deps/cluaiz.exe` with `target/debug` on `PATH` for the DLLs.
- Isolated `CLUAIZ_HOME` (`%LOCALAPPDATA%\cluaiz-test\prefetch-home-run46`), no server, no port.
- Command (`results/steps.log`): `cluaiz bench <model> -p 0 -n 32 -r 1 -t 8 -c 1024 --flash-attn on
  --cache-type-k f16 --cache-type-v f16 -vv`, one run per config.

| Config | Environment |
|---|---|
| A-auto | none (setting Auto) |
| B-auto-cache | `CLUAIZ_MOE_CACHE=on` (run 44's C-pf without the prefetch switch) |
| C-env-on-cache | `CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=on` |

## Machine state: run 44's shape was not there

- Free RAM before the runs: 5,647 MiB (`results/free_ram_before.txt`); run 44 needed the owner to fill RAM,
  this run did not.
- Free VRAM: 1,251 of 4,094 MiB (`results/gpu.txt`); other programs held 2.6 GB. Run 44 had 2.67-2.85 GB.
- So the planner gave no MoE cache in any run (no `MoE cache` line in any log; A-auto planned CPU only,
  B and C a MoE split with 6 and 21 attention blocks on the GPU). Without a cache the prefetch has no upload
  path, so C-env-on-cache could not show the override either.

## Results

| Config | exit | placement | MoE cache | prefetch lines | tg32 t/s |
|---|---|---|---|---|---|
| A-auto | 0 | CPU only, experts read from disk | none | 0 | 3.37 |
| B-auto-cache | 0 | MoE split, ngl 7 | none | 0 | 2.92 |
| C-env-on-cache | 0 | MoE split, ngl 22 | none | 0 | 2.98 |

`grep -ciE "moe cache|moe_prefetch|prefetch "` gives 0 in each log. The speeds are one run each on a busy
laptop and are not a comparison.

## What this shows and what it does not

- Shows: with Auto, a real load on Windows printed no prefetch line, and the bench ran (exit 0 in all three).
- Does not show: Auto turning off for a small cache on a live load (no cache was given), or the env override
  at run time. Those shapes are covered only by the unit tests `auto_follows_the_measured_shapes` (run 42, 43
  and 44 numbers) and the 882-plan check in `engine/plan/src/expert_prefetch.rs`.

## Pending

| Item | Why |
|---|---|
| Live small-cache run with Auto | needs run 44's state: about 2.7 GB of free VRAM and little free RAM |
| Live big-cache run on Linux with Auto | the shape where Auto is on; a run 43 style cloud run |
