# Run 40: Cluaiz and Ollama next to a Node.js website with 20 users, 4 GB and 8 GB machines (2026-11-21/22)

Run 26's checks again with the current Cluaiz build (`b8ef5f64`, sha256 `18c120a3...` in each `manifest.json`),
engines interleaved (Cluaiz, llama.cpp, Ollama, then the same again) so both rounds see the same CPU state.
Scripts: `test/cloud/remote/vps26/` (`run40.sh`, `gcp_setup.sh`, `webapp/`, `compare40.py`). Per-machine
tables: `gcp-4gb/tables.md`, `gcp-8gb/tables.md`, `hostinger-vps-partial/tables.md`. Large files are gzipped.

| | `gcp-4gb` | `gcp-8gb` |
|---|---|---|
| Machine | Google Cloud `e2-custom-2-4096`: 2 vCPU (Xeon 2.20 GHz), 3,908 MB | `e2-standard-2`: 2 vCPU, 7,934 MB |
| OS | Ubuntu 24.04.5, kernel 7.0.0-1017-gcp | same |
| CPU steal | 0.0-2.1% per block (`cpu_stat.log`): no host cap | 0.0% |
| Next to the engine | `webapp/server.js` (Node.js 18, PostgreSQL 16: shop page, product API, order API) with 20 simulated users (`webapp/web_users.js`), about 1,750 requests a minute, for the whole run | same |
| Engines | Cluaiz 0.1.0 pre-release at `b8ef5f64`; Ollama 0.40.16; llama.cpp `b12428` **did not start** (below) | same |
| Settings | 2 threads, context 4096, Q8_0 KV cache, one slot, temperature 0, Qwen3 Q4_K_M | same |
| Load | 90-second steps at 1 and 4 requests at once, two rounds; then Cluaiz W4 / W5 at 1, 2, 4 | same |

**llama.cpp is missing from the GCP results.** `llama-server` exited with `libgomp.so.1: cannot open shared
object file` on the fresh image (the run 26 VPS had it from build-essential), so both llama.cpp blocks show
`FAILED TO START`. `gcp_setup.sh` now installs `libgomp1`. The VMs were deleted after the copy.

## W1 chat, seconds per request (median), both rounds

| Machine | Model | At once | Cluaiz r1 / r2 | Ollama r1 / r2 |
|---|---|---:|---|---|
| 8 GB | Qwen3 0.6B | 1 | 1.35 / 1.32 | 1.32 / 1.31 |
| 8 GB | Qwen3 0.6B | 4 | 5.39 / 5.27 | 4.48 / 4.52 |
| 8 GB | Qwen3 1.7B | 1 | 2.81 / 2.77 | 3.71 / 3.73 |
| 8 GB | Qwen3 1.7B | 4 | 11.39 / 11.28 | 9.93 / 10.15 |
| 4 GB | Qwen3 0.6B | 1 | 1.58 / 1.54 | 1.64 / 1.59 |
| 4 GB | Qwen3 0.6B | 4 | 6.24 / 6.12 | 5.43 / 5.82 |
| 4 GB | Qwen3 1.7B | 1 | 3.28 / 3.22 | 4.52 / 5.85 |
| 4 GB | Qwen3 1.7B | 4 | 13.25 / 13.17 | 12.44 / 13.42 |

Every chat request returned OK on both machines (`tables.md`, column OK). The label accuracy was level
(0.6B 0.27-0.31, 1.7B 0.79-0.88 for both engines).

Read with care:
- One request at a time, Cluaiz was level on 0.6B and faster on 1.7B. With 4 at once, Ollama was faster
  on 0.6B (about 15% on 8 GB) and on 1.7B on 8 GB (about 12%); on 4 GB the 1.7B step was level.
- Answer lengths: both engines answer with one short JSON object (examples: Cluaiz `{"label": "bug"}`,
  Ollama `{\n  "label": "bug"\n}` or `{"label": "outage"}`). The token counts are not comparable: for the
  same text `{\n  "label": "outage"\n}` Cluaiz reports 10 completion tokens and llama.cpp 15 (Hostinger
  data), and Ollama reports 12-15. So the `Gen t/s` column is not a speed comparison; seconds per request is.
- Part of the 1.7B single-request gap may be shorter answers: Cluaiz's 1.7B answers were mostly the compact
  form. Not separated in this run.

## Memory (engine anonymous memory, peak per model)

| Machine | Model | Cluaiz | Ollama |
|---|---|---:|---:|
| 8 GB | Qwen3 0.6B | 534-560 MB | 788 MB |
| 8 GB | Qwen3 1.7B | 1,004 MB | 1,448 MB |
| 4 GB | Qwen3 0.6B | 560 MB | 782-788 MB |
| 4 GB | Qwen3 1.7B | 1,004 MB | 1,440-1,444 MB |

Cluaiz used about 30% less memory for the same model and settings, in every block.

## The website next to the engines

| Machine | Requests | OK | Median | 95th percentile | Slowest |
|---|---:|---:|---:|---:|---:|
| 8 GB | 88,212 | 88,212 | 4 ms | 11 ms | 163 ms |
| 4 GB | 90,111 | 90,111 | 7 ms | 16 ms | 407 ms |

No website request failed or passed 5 s in any step, with either engine (`tables.md`, last column).

## W4 embeddings and W5 speech (Cluaiz): run 26's refusals are gone

| Machine | Workload | 1 at once | 2 at once | 4 at once | HTTP 429 |
|---|---|---|---|---|---:|
| 8 GB | W4 embeddings | 10 / 10 | 12 / 12 | 14 / 14 | 0 |
| 8 GB | W5 speech (TTS + STT) | 17 / 17 | 18 / 18 | 21 / 21 | 0 |
| 4 GB | W4 embeddings | 9 / 9 | 11 / 11 | 13 / 13 | 0 |
| 4 GB | W5 speech | 15 / 15 | 16 / 16 | 19 / 19 | 0 |

In run 26 (build before `fe21984c`) embeddings refused 1,179 of 1,252 requests at 2 at once and speech 101
of 304 at 4 at once. Now the requests wait in line: the median time grows with the queue (8 GB W4: 9.3 s,
17.8 s, 35.5 s), and none is refused.

## Hostinger VPS, before the stop (`hostinger-vps-partial/`)

The first attempt ran on the run 26 VPS (1 vCPU) with 2-minute steps and run26's default 1-hour soak after
each model. The host's CPU credits ran out during llama.cpp's 0.6B soak (steal 0.6% in the Cluaiz block,
38.6% after; Hostinger: sustained use above about 60% for about 3 hours empties the credits), so the run was
stopped. Comparable rows (full CPU, before the cap), Qwen3 0.6B: Cluaiz 0.625 s and llama.cpp 0.629 s per
request at 1 at once; 2.53 s and 2.33 s at 4 at once; first token 0.343 s and 0.315 s. Cluaiz's 1-hour soaks
at full CPU: 5,564 / 5,564 (0.6B) and 2,726 / 2,726 (1.7B) OK. The preflight log showed `2 ms total` per
request (run 26's build spent about 60 ms there, fixed in `ef6a7113`).

## Not proven

- llama.cpp on the GCP machines (did not start; rerun with `libgomp1`).
- Docker: not run.
- Why Ollama is faster at 4 at once with the same one slot: not investigated.
