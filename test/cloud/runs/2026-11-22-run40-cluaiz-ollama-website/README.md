# Cluaiz on small Linux servers: every measurement so far, compared

This page puts together everything measured about running Cluaiz on a small Linux server (1 or 2 vCPU,
4 or 8 GB, no GPU), often next to a website and its database, against llama.cpp and Ollama. It covers
seven runs from 2026-11-16 to 2026-11-22. Every number comes from a run folder in `test/cloud/runs/`;
the source is named in each table. Market facts come from the competitor study in
`competitor-repos/.agent/compare/` (code read at the commits named there). Run 40's own numbers and
method: [SUMMARY.md](SUMMARY.md).

| Run | Date | Machine | What it measured |
|---|---|---|---|
| [15](../2026-11-16-run15-vps-cpu/SUMMARY.md) | 2026-11-16 | Hostinger VPS: 1 vCPU (AMD EPYC 9354P), 3.8 GB | raw speed, Cluaiz `bench` against `llama-bench`, same settings |
| [16](../2026-11-16-run16-vps-ollama/SUMMARY.md) | 2026-11-16 | same VPS | raw speed with Ollama added; Ollama model loading |
| [19](../2026-11-16-run19-vps-deep/SUMMARY.md) | 2026-11-16 | same VPS, a website beside it | 9 chat models, speech, bursts, agent, flows, per-second memory |
| [26](../2026-11-20-run26-vps-load/README.md) | 2026-11-20/21 | same VPS, web app + PostgreSQL | ramps 1-32 at once, 1-hour soaks, native and Docker; a host CPU cap fell between engines |
| [38](../2026-11-21-run38-run26-recheck/SUMMARY.md) | 2026-11-21 | Windows laptop | rechecks of run 26's two bugs on the fixed code |
| [40](SUMMARY.md) | 2026-11-21/22 | Google Cloud 2 vCPU, 4 GB and 8 GB; Node.js website with 20 users | Cluaiz and Ollama interleaved, two rounds; embeddings and speech at 1, 2, 4 at once |
| [40, VPS part](hostinger-vps-partial/tables.md) | 2026-11-21 | the run 15 VPS | first attempt of run 40, stopped when the host's CPU credits ran out |

## The short answer

**Where Cluaiz is ahead, with proof:**

| Point | Numbers | Source |
|---|---|---|
| Less memory for the same model and settings | engine memory: Qwen3 0.6B 534-560 MB against Ollama 782-788 MB; Qwen3 1.7B 1,004 MB against 1,440-1,448 MB (about 30% less, every block, both machines) | run 40 |
| Lowest machine memory in a 1-hour chat soak, with twice the queue | Cluaiz 1,759 MB with 32 requests in flight; llama.cpp 1,811 MB and Ollama 2,055 MB with 16 | run 26 (all three under the same CPU cap) |
| No failed chat request in any run | run 40: every step OK on both machines; run 26 soak: 1,048 / 1,048; run 40 VPS soaks: 5,564 / 5,564 and 2,726 / 2,726 | runs 26, 40 |
| The website next to it is not affected | 88,212 / 88,212 and 90,111 / 90,111 requests OK, 95th percentile 11 and 16 ms | run 40 |
| Chat, embeddings, text to speech and speech to text in one process, under load | 1,063 / 1,063 requests in one hour (698 chat, 204 embeddings, 81 TTS, 80 STT), machine peak 2,434 MB of 3,915 | run 26 |
| Many embedding and speech requests at once are queued, not refused | 0 HTTP 429 at 1, 2 and 4 at once on both machines (run 26's build refused 1,179 of 1,252 embeddings at 2 at once) | runs 26, 38, 40 |
| Loads where Ollama did not | Llama 3.2 3B with F16 cache on 3.8 GB: Cluaiz completed, Ollama's runner was killed at 1.6 GB free | run 16 |

**Where Cluaiz is level:**

| Point | Numbers | Source |
|---|---|---|
| Raw speed against official llama.cpp | same settings, 1 thread: prompt and generation within the run-to-run spread on Q1_0 and Q4_K_M (Llama 3.2 3B: 21.17 / 8.16 t/s against 21.80 / 7.60) | run 15 |
| One chat request at a time, small model | Qwen3 0.6B: Cluaiz 0.625 s, llama.cpp 0.629 s (full CPU); Cluaiz 1.32-1.35 s, Ollama 1.31-1.32 s (8 GB) | runs 40, 40 VPS |
| Answer quality | right label: Qwen3 0.6B 0.27-0.31 and 1.7B 0.79-0.88 for both engines; valid JSON in every step | runs 26, 40 |

**Where Cluaiz is behind or unproven:**

| Point | Numbers | Source |
|---|---|---|
| 4 requests at once | Ollama faster on 8 GB: 0.6B 4.48-4.52 s against 5.27-5.39 s (about 15%), 1.7B 9.93-10.15 s against 11.28-11.39 s (about 12%); llama.cpp faster on the VPS 0.6B: 2.33 s against 2.53 s | runs 40, 40 VPS |
| One quantization slower than llama.cpp | Ternary-Bonsai Q2_0: 13-20% slower | run 16 |
| Time to first token, single request | Cluaiz 0.343 s against llama.cpp 0.315 s (0.6B, full CPU) | run 40 VPS |
| CLI timeouts on a busy 1 vCPU | agent probe and `flow run` timed out | run 19 |
| Not measured yet | llama.cpp on the Google machines (it did not start), Docker since run 26, Mac, more than 4 at once since run 26 | run 40 |

In one line: on a small server, Cluaiz runs chat at the same speed as llama.cpp and Ollama one request
at a time, uses about 30% less memory than Ollama, never failed a request, kept the website next to it
untouched, and runs embeddings and speech in the same process. With several requests at once it is
about 12-15% slower than Ollama today.

## Angle 1: raw speed, same settings

`cluaiz bench` against `llama-bench` and Ollama, 1 thread, prompt 512, generation 128, 3 runs, on the
1 vCPU VPS (runs 15 and 16). Prompt / generation in tokens per second.

| Model | KV cache | Cluaiz | llama.cpp `b12428` | Ollama 0.40.16 |
|---|---|---|---|---|
| Bonsai 4B Q1_0 | F16 | 7.82 / 6.49 | 7.77 / 6.56 | 7.27 / 4.99 |
| Bonsai 4B Q1_0 | Q8_0 | 6.75 / 5.79 | 6.71 / 5.99 | 6.64 / 4.50 |
| Llama 3.2 3B Q4_K_M | F16 | 21.17 / 8.16 | 21.80 / 7.60 | did not load |
| Llama 3.2 3B Q4_K_M | Q8_0 | 17.10 / 7.86 | 16.71 / 7.73 | 18.84 / 6.74 |
| Ternary-Bonsai-2-4B Q2_0 | F16 | 5.51 / 4.25 | 6.54 / 4.86 | 5.77 / 4.32 |

Reading: Cluaiz runs llama.cpp inside, so the same model at the same settings runs at the same speed;
that is the design, and the public claim is "same speed", never "faster". Ollama generated slower on
these two models. The Q2_0 row is the one case where Cluaiz was clearly behind llama.cpp.

## Angle 2: chat under real load

W1: classify a support ticket into one label, answer as JSON (`remote/vps26/workloads.py`). Median
seconds per request, the measure a user feels. Run 40 ran each engine twice, interleaved, so both saw
the same machine state (CPU steal 0.0-2.1% on Google Cloud).

| Machine | Model | At once | Cluaiz r1 / r2 | Ollama r1 / r2 | llama.cpp |
|---|---|---:|---|---|---|
| GCP 8 GB, 2 vCPU | Qwen3 0.6B | 1 | 1.35 / 1.32 | 1.32 / 1.31 | did not start |
| GCP 8 GB | Qwen3 0.6B | 4 | 5.39 / 5.27 | 4.48 / 4.52 | did not start |
| GCP 8 GB | Qwen3 1.7B | 1 | 2.81 / 2.77 | 3.71 / 3.73 | did not start |
| GCP 8 GB | Qwen3 1.7B | 4 | 11.39 / 11.28 | 9.93 / 10.15 | did not start |
| GCP 4 GB, 2 vCPU | Qwen3 0.6B | 1 | 1.58 / 1.54 | 1.64 / 1.59 | did not start |
| GCP 4 GB | Qwen3 0.6B | 4 | 6.24 / 6.12 | 5.43 / 5.82 | did not start |
| GCP 4 GB | Qwen3 1.7B | 1 | 3.28 / 3.22 | 4.52 / 5.85 | did not start |
| GCP 4 GB | Qwen3 1.7B | 4 | 13.25 / 13.17 | 12.44 / 13.42 | did not start |
| VPS 1 vCPU, full CPU | Qwen3 0.6B | 1 | 0.625 | not reached | 0.629 |
| VPS 1 vCPU, full CPU | Qwen3 0.6B | 4 | 2.53 | not reached | 2.33 |

Time to first token, median, single request: GCP 8 GB 0.6B Cluaiz 0.71-0.73 s, Ollama 0.74-0.77 s;
VPS 0.6B Cluaiz 0.343 s, llama.cpp 0.315 s.

Reading:
- One request at a time the engines are level on 0.6B. On 1.7B Cluaiz finished each request sooner
  (2.8 s against 3.7 s on 8 GB). Part of that may be shorter answers: Cluaiz's 1.7B answers were mostly
  the compact `{"label": "bug"}`, Ollama's sometimes spread over three lines. Not separated in this run.
- With 4 at once, Ollama was faster. Both run one slot, so the requests queue. Where the time goes
  (8 GB, round 1, Qwen3 0.6B, medians from `requests_native_*.jsonl`, first request of a step left out):

  | Engine | At once | Answer | First token | After first token | Total | Requests in 90 s |
  |---|---:|---|---:|---:|---:|---:|
  | Cluaiz | 1 | 23 characters | 0.723 s | 0.618 s | 1.342 s | 64 |
  | Cluaiz | 4 | 23 characters | 4.765 s | 0.625 s | 5.387 s | 71 |
  | Ollama | 1 | 23 characters | 0.729 s | 0.627 s | 1.312 s | 70 |
  | Ollama | 4 | 23 characters | 3.815 s | 0.639 s | 4.484 s | 83 |

  The model work is the same: one request at a time, the same 23-character answer took the same time to
  its first token and the same time after it on both engines. With 4 clients and one slot a request waits
  for the other three, so the total is about four times the time the slot is busy per request. For
  Cluaiz 5.387 s is 4.0 times 1.342 s: the slot is busy for the whole request. For Ollama 4.484 s is 3.4
  times 1.312 s: about 0.19 s of each request happens outside the slot, overlapped with the previous one.
  Which part of Cluaiz's request is still inside the slot (request parsing, the chat template, tokenizing,
  or the work after the last token) is not measured; the next step is to log when the slot is taken and
  released for one request.
- The same 23 characters were counted as 10 completion tokens by Cluaiz and 15 by Ollama (and 15 by
  llama.cpp in the Hostinger data) while taking the same time, so Cluaiz's `completion_tokens` for this
  answer looks low; not checked against the tokenizer yet.
- The 4 GB machine was slower than the 8 GB one for both engines (same CPU type), which points at memory
  pressure or the 20-user website taking a larger share; not separated.

## Angle 3: memory

| Measure | Cluaiz | llama.cpp | Ollama | Source |
|---|---:|---:|---:|---|
| Engine memory, Qwen3 0.6B (GCP 8 GB) | 534-560 MB | - | 788 MB | run 40 |
| Engine memory, Qwen3 1.7B (GCP 8 GB) | 1,004 MB | - | 1,448 MB | run 40 |
| Engine memory, Qwen3 0.6B (VPS, full CPU) | 559 MB | 678 MB | - | run 40 VPS |
| Engine memory, Qwen3 1.7B (VPS) | 1,002 MB | 1,145 MB | - | run 40 VPS |
| Machine peak, 1-hour chat soak, Qwen3 1.7B | 1,759 MB (32 in flight) | 1,811 MB (16) | 2,055 MB (16) | run 26 |
| Engine anonymous memory peak, same soak | 1,139 MB | 1,147 MB | 1,554 MB | run 26 |
| Llama 3.2 3B, F16 cache, 3.8 GB machine | loaded and ran | ran | runner killed (1.6 GB free) | run 16 |

Reading: against llama.cpp the difference is small (about 50-140 MB); against Ollama it is about 30%.
On a 4 GB server that shares memory with a database, 450 MB is the difference between a second model or
a bigger context fitting or not.

## Angle 4: reliability

| Measure | Cluaiz | llama.cpp | Ollama | Source |
|---|---|---|---|---|
| Failed chat requests, all W1 steps | 0 | 0 | 0 | runs 26, 40 |
| 1-hour chat soak | 1,048 / 1,048 | 1,073 / 1,073 | 1,102 / 1,102 | run 26 |
| Mixed load, 1 hour (Cluaiz with speech; Ollama without) | 1,063 / 1,063 | - | 15 embedding timeouts (native), 15 (Docker) | run 26 |
| Embeddings, 2 at once | run 26 build: 1,179 of 1,252 refused; fixed build: 0 refused | - | - | runs 26, 40 |
| Speech, 4 at once | run 26 build: 101 of 304 refused; fixed build: 0 refused | - | - | runs 26, 40 |
| OOM kills | none | none | Llama 3.2 3B F16 runner killed | runs 16, 26 |
| CLI under a busy 1 vCPU | agent probe and `flow run` timed out | - | - | run 19 |

## Angle 5: the app next to the engine

The point of a small server is that it already runs something. Each run kept an app beside the engine.

| Run | App | Checks | Result |
|---|---|---|---|
| 19 | website container | every check | all answered |
| 26 | Python web app + PostgreSQL, one check a second | mixed soak 3,492; chat soaks about 3,500 each | Cluaiz mixed 3,492 / 3,492; Cluaiz chat soak 3,559 / 3,560 (one 5 s timeout); llama.cpp and Ollama 0 failures |
| 40 | Node.js shop (page, product API, order API) + PostgreSQL, 20 simulated users, about 1,750 requests a minute | 88,212 (8 GB), 90,111 (4 GB) | all OK; median 4 and 7 ms, 95th percentile 11 and 16 ms, slowest 163 and 407 ms |

With either engine busy, no website request in run 40 failed or passed 5 s.

## Angle 6: embeddings and speech at once (Cluaiz only)

| Machine | Workload | 1 at once | 2 at once | 4 at once | HTTP 429 | Median seconds (1 / 2 / 4) |
|---|---|---|---|---|---:|---|
| GCP 8 GB | W4 embeddings | 10 / 10 | 12 / 12 | 14 / 14 | 0 | 9.3 / 17.8 / 35.5 |
| GCP 8 GB | W5 speech | 17 / 17 | 18 / 18 | 21 / 21 | 0 | 5.1 / 10.5 / 17.1 |
| GCP 4 GB | W4 embeddings | 9 / 9 | 11 / 11 | 13 / 13 | 0 | 10.2 / 19.5 / 38.4 |
| GCP 4 GB | W5 speech | 15 / 15 | 16 / 16 | 19 / 19 | 0 | 6.0 / 12.2 / 21.4 |

The time doubles with each doubling of requests: they wait in line (up to 120 s) instead of being
refused. llama.cpp and Ollama were not given speech steps in this harness. Their code has speech routes
now (`llama-server` `/v1/audio/transcriptions` and `/v1/audio/speech`; Ollama through its MLX runner;
competitor study, `players/llama-cpp.md`, `players/ollama.md`), so "only Cluaiz does speech" is not a
claim we make; "chat, embeddings and speech in one process on a 4 GB CPU server, measured" is.

## Angle 7: answer quality and token counts

| Measure | Cluaiz | Ollama | llama.cpp | Source |
|---|---|---|---|---|
| Right label, Qwen3 0.6B | 0.28-0.31 | 0.27-0.30 | 0.28-0.29 | runs 40, 40 VPS |
| Right label, Qwen3 1.7B | 0.83-0.86 | 0.79-0.88 | 0.84 | runs 40, 40 VPS |
| Valid JSON | every step | every step | every step | runs 26, 40 |
| Reported completion tokens for `{\n  "label": "outage"\n}` | 10 | 12-15 | 15 | runs 40, 40 VPS |

The model, not the engine, decides the quality: 0.6B picked the right label about 3 times in 10, 1.7B
about 8 in 10, on every engine. Token counts are not comparable between engines (the same text is
counted as 10 or 15), so tokens per second is no fair speed measure here; seconds per request is.

## Angle 8: what each product gives beyond chat

From the competitor study (`compare/angles/README.md`, `compare/CONCLUSION.md`; "yes" = found in code).
Only rows that matter on a small server.

| Capability | Cluaiz | Ollama | llama.cpp | LocalAI | AnythingLLM / Open WebUI |
|---|---|---|---|---|---|
| Runs the models itself | yes, one process (llama.cpp + ONNX) | yes | yes | yes, process per backend | no, needs a runner beside it |
| Speech routes | yes, measured here | yes (MLX runner) | yes | yes | yes |
| Memory | yes, user-confirmed | no | no | yes (dev builds) | yes |
| MCP client | yes | no | web UI proxy | yes | yes |
| Plugins with run-time limits | WASM, fuel and memory limits | no | no | add-ons unsandboxed | ? |
| OS sandbox for agent commands | yes on `main` (Linux, macOS; Windows opt-in), not in v0.1.0 (run 41) | no | no | add-on (Linux) | no / ? |
| Automation, flows | yes, built, not released | no | no | jobs (dev) | flows / automations |
| Ollama-style API | yes (`/api/chat`, `/api/generate`, `/api/embed`, `/api/tags`, ...; run 36) | native | no | ? | - |
| Multi-user | no | no | no | yes | yes |

Reading: no single feature is unique. The combination in one process (models, speech, memory, tools,
an agent with a sandbox, one permission system) is rare; LocalAI dev builds come closest
(`compare/CONCLUSION.md` section 3). On a small server that matters because every extra service
(a runner, a vector database, a speech server, a workflow runner) takes memory and upkeep.

## Where Cluaiz sits

| If you need | Best fit today | Why |
|---|---|---|
| One model, one API, nothing else, already set up | Ollama or llama.cpp | familiar, fewer moving parts; Ollama was faster with 4 requests at once |
| Chat plus embeddings plus speech on a 4 GB server beside an app | Cluaiz | measured in one process with the website untouched, about 30% less memory than Ollama |
| Agent commands that must not read your secrets | Cluaiz on `main`, Codex, Jan | OS sandbox with real-machine checks (runs 27, 28, 41) |
| Many users, teams, image generation | Open WebUI, AnythingLLM, LocalAI | Cluaiz has no multi-user and no image generation |
| Heavy concurrency on a big machine | not measured for Cluaiz | only up to 32 in flight on 1 vCPU (run 26) |

## What went wrong in the tests themselves

| Problem | Effect | Fix |
|---|---|---|
| Hostinger KVM 1 runs on CPU credits: sustained high use for about 3 hours drops it to 40% | run 26 compared Cluaiz at full CPU with the others capped (an early "2.6x faster" summary was wrong); run 40's VPS attempt hit the cap during llama.cpp's soak | engines interleaved; steal time logged per block (`cpu_stat.log`); dedicated-CPU machines (Google `e2-standard`) |
| `run26.sh` adds a 1-hour soak after each model by default | run 40's first attempt took about 2 hours per engine block | `run40.sh` passes `--soak-secs 0` unless asked |
| Fresh Ubuntu image without `libgomp1` | llama.cpp did not start on Google Cloud | `gcp_setup.sh` installs it |
| Token counts differ per engine | tokens per second misleads | compare seconds per request; answer texts saved |

## Not proven yet

- llama.cpp on the Google machines; Docker since run 26; more than 4 requests at once since run 26.
- Which part of a Cluaiz chat request keeps the slot busy (Angle 2: about 0.19 s per request that
  Ollama overlaps with the previous request).
- Metal GPU speed on real Apple hardware after `07cf30fa` (the October M5 Air run,
  `test/mac/runs/2026-10-28-m5-air-16gb/`, ran on the CPU because the engine did not use Metal then).
- Bigger models (3B and up) under the website load on 4 GB.

## Rerun

Prepare a fresh Ubuntu 24.04 VM with `test/cloud/remote/vps26/gcp_setup.sh` (needs the bundle described
at its top), then:

```bash
CLUAIZ_COMMIT=<sha> ROUNDS="1 2" STEP_SECS=90 WEB_USERS=20 bash /root/vps26/run40.sh /root/results-run40
python3 test/cloud/remote/vps26/compare40.py /root/results-run40 > tables.md
```

## Charts

`images/` holds the greyscale charts used in the blog draft and the Reddit post, drawn from the numbers in
this README and run 26: `python images/run40_charts.py` (charts 1 to 4) and `python images/run40_charts2.py`
(Docker, 4 GB against 8 GB, every test in one table) redraw them next to the scripts. The charts carry no
paths or links, so a moved folder does not make them wrong; the post text carries the links.
`8-setup-one-server.png` was drawn separately with Pillow.
