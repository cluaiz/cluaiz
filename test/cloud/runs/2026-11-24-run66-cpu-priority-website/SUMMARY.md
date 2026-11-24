# Run 66: lower CPU priority for the engine, next to a website on one CPU (2026-11-24)

Question from Reddit readers: if the engine runs with `nice -n 19` or systemd `CPUWeight=10`, does the website
next to it answer faster, and how much slower does the model get? Run 26 never tried it.

**Answer on this machine:** yes. The website's 95th percentile fell from 19.3 / 19.2 ms (no change, before and
after) to 10.7 ms with `nice -n 19` and 10.1 ms with `CPUWeight=10`, and its slowest check from 110 / 319 ms
to 16 / 20 ms. The model did not get measurably slower: 2.70 tokens/s and 6.78-6.80 generation t/s in every
arm. The engine still used 97.8-97.9% of the CPU; the website needs so little that giving it the CPU first
costs the model nothing that shows in these numbers.

## Setup

| | |
|---|---|
| Machine | Google Cloud VM `cluaiz-32gb` (g2-standard-8): Intel Xeon @ 2.20 GHz, 8 vCPU (4 cores x 2 threads), 31 GB, Debian 13, kernel 6.12.117 (`results/main/machine.txt`) |
| One CPU | **A pinned single core on a bigger VM, not a real 1 vCPU VPS.** Everything below ran on cpu 2 only (cgroup cpuset `AllowedCPUs=2`); its hyperthread sibling cpu 6 was not reserved and the other 7 vCPU were free for the system. Checked per arm in `scope_check.txt` (`cpuset.cpus.effective: 2`, column PSR = 2) |
| On cpu 2 | the engine (systemd scope, memory limit 3,515 MB as run 26's manifest, no swap); PostgreSQL 17 (`systemctl set-property --runtime postgresql@17-main.service AllowedCPUs=2`); the harness as a system service (`run66.service`): run 26's load client `workloads.py`, the website `neighbour_app.py` and the once-a-second checker in `sampler.py` |
| GPU | off: the Engine was built CPU only (`--no-default-features`, no CUDA library); the log says "GPU offload not available, models run on the CPU", 28 of 28 blocks in RAM. `CUDA_VISIBLE_DEVICES` empty |
| Engine | Cluaiz 0.1.0 pre-release, `cargo build -p cmd --release --no-default-features` on the VM at `961a673f` (llama.cpp fork `96b0cfb2d`), SHA-256 `de29e2e7...` in each `manifest.json`. Engine code is the same as run 63's `9dfd4f46` (only `test/` changed in between) |
| Other engines | official llama.cpp `b12428` (Ubuntu x64 CPU release), Ollama 0.40.16 (`num_gpu 0`, `CUDA_VISIBLE_DEVICES` empty) |
| Model and settings | Qwen3 1.7B Q4_K_M (`unsloth/Qwen3-1.7B-GGUF`, sha256 `b139949c...`), 1 thread, context 4096, Q8_0 KV cache, one slot, temperature 0, thinking off: run 26's settings |
| Load | run 26's Cluaiz soak: W1 ticket labels as JSON, 32 requests in flight (all engines; run 26 used 16 for llama.cpp and Ollama), run 26's own `w1_tickets.json` (same md5), 1,200 s per arm plus the drain of the last 32 requests (about 70 s) |
| Website | `neighbour_app.py` with PostgreSQL: each `GET /health` does one indexed read and one write; schema and 5,000 rows as `webapp/server.js`. Checked once a second; the checker gives up at 5 s |
| Arms, in order | website alone (2 min, no engine); A1 no change; B `nice -n 19`; C `CPUWeight=10` on the engine scope (website and PostgreSQL at the default 100); A2 no change again; then llama.cpp and Ollama with `nice -n 19` |
| Priority switch | `engines.sh` `ENGINE_PRIORITY` (none / nice / cpuweight), `run26.sh --priority`, default unchanged. Proof per arm in `scope_check.txt`: B shows NI 19 on every engine thread, C shows `CPUWeight=10` and `cpu.weight: 10` |

Why the harness is a system service: on a VPS the website is a service too. The engine scope, PostgreSQL and the
website then sit side by side in `system.slice`. On this machine `system.slice` had no cpu controller of its
own (`cpu_stat.log`: `cpuset memory pids`), so all of them shared one CPU group and `nice` worked between them.
`CPUWeight=10` turned the cpu controller on for `system.slice` only while the C scope existed (seen in C's
`scope_check.txt`); it was off again at A2's start, so A2 and the nice arms after C were not changed by C.

## Results

`python3 summarize_run66.py results/main` (and `results/extra` for the llama.cpp and Ollama rows). Website
columns cover only each arm's soak phase.

| Arm | Engine | Priority | Website checks | Median ms | p95 ms | Max ms | Over 5 s | Answers OK | Tokens/s (usage) | s per request | Median gen t/s (engine log) | Usage chunk | Machine memory peak MB | Engine anon peak MB | Engine CPU avg % | Steal % |
|---|---|---|---:|---:|---:|---:|---:|---|---:|---:|---:|---|---:|---:|---:|---:|
| website alone | - | - | 119 | 6.5 | 7.0 | 24.6 | 0 | - | - | - | - | - | 1156 | - | - | 0.00 |
| A1 | cluaiz | none | 1256 | 15.0 | 19.3 | 110.2 | 0 | 555 / 555 | 2.70 | 2.30 | 6.78 (n=555) | 555 / 555 | 2422 | 1138 | 97.9 | 0.00 |
| A2 | cluaiz | none | 1255 | 15.1 | 19.2 | 318.6 | 0 | 555 / 555 | 2.70 | 2.29 | 6.79 (n=555) | 555 / 555 | 2426 | 1173 | 97.8 | 0.00 |
| B | cluaiz | nice | 1266 | 9.0 | 10.7 | 16.4 | 0 | 556 / 556 | 2.71 | 2.29 | 6.79 (n=556) | 556 / 556 | 2319 | 1057 | 97.9 | 0.00 |
| C | cluaiz | cpuweight | 1264 | 6.7 | 10.1 | 20.1 | 0 | 556 / 556 | 2.71 | 2.29 | 6.80 (n=556) | 556 / 556 | 2360 | 1083 | 97.8 | 0.00 |
| website alone (before L, O) | - | - | 119 | 6.6 | 6.9 | 24.9 | 0 | - | - | - | - | - | 1219 | - | - | 0.00 |
| L | llama | nice | 1272 | 9.0 | 10.0 | 29.5 | 0 | 498 / 498 | 5.00 | 2.57 | 6.44 (n=498) | 498 / 498 | 2422 | 1142 | 97.9 | 0.00 |
| O | ollama | nice | 1271 | 9.1 | 10.6 | 305.5 | 0 | 501 / 501 | 5.03 | 2.55 | 6.51 (n=501) | 501 / 501 | 2816 | 1565 | 97.9 | 0.00 |

Columns: "Over 5 s" counts checks that took 5 s or more or did not return HTTP 200. Tokens/s = completion
tokens from the usage chunk of every answer / soak wall time. Seconds per request = wall time / answers (one
slot, so this is the time per answer, not the wait in the queue, which was 67-72 s at 32 in flight). Generation
t/s is the median of llama.cpp's own per-request timing line in the engine log (Ollama's runner prints the same llama.cpp line). Machine
memory is the whole 31 GB VM (about 1.1 GB used before the test); engine anonymous memory is the engine alone.
Steal is cpu 2's steal ticks over all its ticks in the arm.

llama.cpp and Ollama were run only with `nice -n 19` (the plan: only if B or C helped). Their website p95
(10.0 and 10.6 ms) is at the level of Cluaiz under nice (10.7 ms); there is no no-priority arm for them, so
their own drop is not measured. Ollama had one slow check (305 ms) in the arm; still 0 over 5 s.

Their tokens/s are not comparable with Cluaiz: the answers differ in length. Median completion tokens per
answer, from the usage chunk: Cluaiz 6 (`{"label": "bug"}` on one line), llama.cpp 14 (`{\n  "label": ...\n}`,
pretty-printed). Generation speed per token was close: 6.78-6.80 t/s Cluaiz, 6.44 llama.cpp, 6.51 Ollama
(engine logs), and seconds per answer 2.29-2.30 against 2.57 and 2.55, at 32 in flight (run 26 soaked these two
at 16).

## What this does not prove

- **A real 1 vCPU VPS.** This is one pinned hyperthread of an 8 vCPU VM. Run 26 on a real 1 vCPU VPS saw a
  website p95 of 173 ms with no priority change; here it was 19 ms. The VPS had a host CPU cap (run 26
  summary) and a different CPU, kernel and scheduler setup, so the size of the drop there is unknown.
- **Other cgroup layouts.** If the website runs from a login session (user.slice) or in Docker, the root cgroup
  splits the CPU by slice first, and `nice` on the engine may not reach across; `CPUWeight` then needs to be
  set on the right parent. Not tested.
- **A busier website.** One check a second is a very light website. A website that needs much CPU would take
  more from the engine under `nice` and the model cost would show.
- **Length.** 20 minutes per arm, not run 26's 1 hour. A1 and A2 agree (p95 19.3 / 19.2 ms), so the machine
  state did not drift during the run.
- Steal was 0.00% in every arm, so no host took CPU back; that is not true of every VPS.

## Files

- `run66.sh` (the arms), `setup66.sh` (VM setup), `build66.sh` (Engine build), `check_scope.sh` (per-arm
  scope proof), `summarize_run66.py` (the table). The harness is `test/cloud/remote/vps26/` at this commit.
- `results/main/`: `run66.log`, `cpu_stat.log`, `machine.txt`, `baseline/`, and one folder per arm with
  run 26's files (`resources.csv` per second, `requests_native_*.jsonl` per answer, `steps_*.jsonl`,
  `manifest.json`, engine log, `scope_check.txt`) and the arm's console output (`*.out`).
- `results/smoke/`: a 60 s nice arm used to check the setup; not a result.
- `results/extra/`: the llama.cpp and Ollama nice arms (with their own 2-minute website-alone baseline).

## Exact commands (on the VM, as root)

```
bash setup66.sh                          # PostgreSQL, model, llama.cpp b12428, Ollama 0.40.16
bash build66.sh                          # as the VM user: Engine at the bundled HEAD, CPU only
systemd-run --unit=run66 -p AllowedCPUs=2 --setenv=HOME=/root --setenv=CUDA_VISIBLE_DEVICES= \
  --setenv=CLUAIZ_COMMIT=961a673f bash /root/r66/run66.sh /root/results-run66
systemd-run --unit=run66x -p AllowedCPUs=2 --setenv=HOME=/root --setenv=CUDA_VISIBLE_DEVICES= \
  --setenv=CLUAIZ_COMMIT=961a673f "--setenv=ARMS=L:llama:nice O:ollama:nice" \
  bash /root/r66/run66.sh /root/results-run66-extra
```

`HOME` must be set: without it `ollama serve` did not start under systemd and the first smoke run stopped at
run 26's Ollama model sync.
