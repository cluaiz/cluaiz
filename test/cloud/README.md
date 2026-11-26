<!-- name: test/cloud
description: Cloud machine runs: creating test VMs, running suites, and recording every run with its raw results. -->

# Cloud machine runs (`test/cloud/`)

Run the Engine on machines other than the developer's (Google Cloud GPU and CPU VMs, a 1 vCPU VPS, the dev
laptop for same-settings comparisons) and keep every result with its raw output. Speed, memory and platform
claims in READMEs and docs point to a run here.

## At a glance

- **Platform support:** the helper `vm.sh` needs `gcloud` (Bash; Git Bash on Windows). Remote scripts
  run on Ubuntu / Debian VMs; some runs were on Windows (runs 17, 20, 22, 27, 29-31).

## Flow of one run

![Cloud run: vm.sh create, build, test, vm.sh get, run folder, test/benchmarks file, vm.sh delete](../../docs/assets/contributing/cloud-run.png)

`vm.sh` creates Spot VMs (`--provisioning-model=SPOT --instance-termination-action=STOP
--max-run-duration`), tries the zones in `ZONES` in order and remembers the zone per VM. A stopped VM
keeps its disk with the build and the models, so `start` continues; `delete` removes both.

## Files

| Path | What it is |
|---|---|
| `vm.sh` | create / start / stop / ssh / put / get / delete / list for Spot VMs |
| `runs/<date>-runN-<what>/` | one folder per run: raw logs, raw results and the script used, no `.md` files; the write-up is in `test/benchmarks/<test-type>.md` |
| `remote/linux_build.sh`, `linux_tests.sh` | standard build and test on a fresh Ubuntu Deep Learning VM |
| `remote/checks.py` | every chat model (short and long prompt), speech round trip, embeddings, GGUF speech, mixed rounds |
| `remote/regression_tests.sh`, `regression_all.sh` | `test/engine_regression/run.py` on a VM, one model at a time so a stopped Spot VM keeps finished reports |
| `remote/release_tests.sh` | a GitHub release on a clean VM through the one-command install |
| `remote/compare3.py` | Cluaiz, Ollama and LM Studio on one VM with the same GGUF files and prompt |
| `remote/run11.sh`, `run12.sh`, `features11.py` | runs 11-12 on the L4 VM |
| `remote/run21_planner.sh`, `multiturn_cache.py`, `vram_hog.py` | run 21: planner release check, cache reuse past a sliding window, another process taking VRAM during a load |
| `remote/moe_cache_*.sh` | run 13: official llama.cpp's MoE GPU cache and its tuner |
| `remote/ssd_test.sh`, `ssd_same_settings.sh` | a model bigger than RAM + VRAM; Cluaiz against official llama.cpp with the same settings |
| `remote/vps_cpu_same_settings.sh`, `vps_run16_ollama.sh`, `vps_run18_features.sh`, `vps19/` | runs 15-19 on a 1 vCPU / 4 GB VPS without a GPU |
| `remote/mtp23/` | runs 23-24: MTP off, on and Auto in every placement |
| `remote/vps26/` | run 26: Cluaiz, llama.cpp and Ollama next to a web app and PostgreSQL on the 1 vCPU VPS, native and Docker (`run26.sh`, `workloads.py`, `analyze.py`); `run40.sh` reruns its checks with the engines interleaved, the answer texts saved and the Cluaiz commit recorded; `gcp_setup.sh` prepares a fresh Ubuntu VM, `webapp/` is a Node.js + PostgreSQL website with simulated users (`WEB_USERS`), `compare40.py` writes the tables; `run26.sh --priority nice|cpuweight` (and `--engine-cpus`) start a native engine under `nice -n 19` or with systemd `CPUWeight=10` (run 66), default unchanged |
| `remote/ssd_prefetch/` | written, not run: the expert prefetch speed proof on the L4 (gpt-oss-120b, official llama-bench baselines, Cluaiz with the MoE cache forced on and the prefetch off / on, page cache dropped before each run, 3 rounds), and `run_prefetch_laptop.ps1`, run 20 repeated on the Windows laptop with the MoE cache and the prefetch (format-checked on OLMoE only); its README has the commands and the proposed pass bar |
| `remote/warm_tg_gap/run_warm_tg_gap.ps1` | run 47 (Windows laptop): official llama-bench against one or more `cluaiz bench` builds or llama-bench beside other ggml DLLs, same placement, rotated rounds, disk reads and free RAM per run; usage in its header and in `engine/llama/README.md` |
| `remote/models*.txt` | model lists the scripts pull |

## Rules for a run

- An isolated server: `CLUAIZ_HOME` in a temp folder, port 8011. Never a developer's own home or 8000.
- Speeds are llama.cpp's own timings (the `llama.cpp timings` log line or `engine.timings` in the
  response), not counted by the client.
- A comparison with another tool uses the same machine, GGUF file, context, placement and prompt, and
  says which settings differ.
- Raw output is saved to files in the run folder while the test runs; the results are written into
  `test/benchmarks/<test-type>.md` from those files. When a newer run covers the same case, the old numbers
  and the old run folder are deleted.
- Usernames, keys and the VPS address are removed (`<user>`, `<key>`, `<vps>`) before a commit.
- Each benchmark section ends with what the run does not prove and a link to the run folder.

## Decisions
| Decision | Why | Alternative not taken |
|---|---|---|
| Spot VMs that stop themselves | GPU VMs cost money per hour; a forgotten VM stops after `MAX_RUN` | on-demand VMs |
| One model per step in long scripts (`regression_all.sh`) | a Spot reclaim mid-run would lose all results; finished models keep their reports | one long loop |
| Every run in its own folder with its logs and script; one write-up file per test type in `test/benchmarks/` with only the latest results | a reader finds everything in one file; doubters check the logs; no duplicate or outdated numbers | a `SUMMARY.md` per run plus a list of all runs (used until 2026-11-26) |

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| llama.cpp has no row in run 40's GCP results | `llama-server` needed `libgomp1`, which the fresh image lacked (now in `gcp_setup.sh`); Docker was not run | rerun `remote/vps26/run40.sh` on a fresh VM after `gcp_setup.sh` |
| `remote/vram_hog.py` and `remote/mtp23/vram_hog.py` are identical | two copies drift | keep one; `mtp23/run23.sh` and `run24.sh` call `$HERE/vram_hog.py`, so first check how `mtp23/` is copied to the VM |
| A CPU-only run on an 8 GB VM for the Auto thinking ceiling | the slow-machine path is unit-tested only (runs 30-31 were on a GPU) | the planned VM run |
