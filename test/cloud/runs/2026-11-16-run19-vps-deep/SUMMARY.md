# Run 19: deep test on a 1 vCPU / 3.8 GB VPS without a GPU, with a resource log (2026-11-16)

| | |
|---|---|
| Machine | 1 vCPU (AMD EPYC 9354P, AVX-512), 3.8 GB RAM, no swap, no GPU, Ubuntu 24.04; a website container runs beside it (about 0.6 GB of the machine's memory in use before Cluaiz starts) |
| Cluaiz | v0.1.0 re-published release (tag `eb11d47d`); server on 127.0.0.1:8011 in a 2.9 GB memory scope |
| Scripts | [`vps19/`](../../remote/vps19/): `run19.sh` (phases), `sampler.py` (one row per second), `workloads.py`, `analyze.py` |
| Records | [`results-run19/`](results-run19/): `resources.csv` (1,556 seconds), `phases.md`, `results.jsonl`, `steps.log`, every log |

The first attempt ([`results-run19-attempt1/`](results-run19-attempt1/)) is not used: run 18's server was still
running, so the planner saw too little free RAM and most phases failed. The scripts now stop the server by the
PID that listens on its port.

## Memory and CPU per phase

From [`phases.md`](results-run19/phases.md). "Anon" is memory the Cluaiz processes really hold; "mapped" is the
model file mapped into memory, which the kernel can drop and read again. Machine memory includes the website
and the page cache that counts as used.

| Phase | Machine memory peak | Cluaiz anon peak | Cluaiz mapped peak | Cluaiz CPU avg |
|---|---|---|---|---|
| Before Cluaiz | 619 MB | 0 | 0 | 0% |
| Server idle, no model | 634 MB | 9 MB | 35 MB | 0% |
| Qwen3 0.6B chat | 1.33 GB | 741 MB | 414 MB | 92% |
| Llama 3.2 1B chat | 1.24 GB | 661 MB | 813 MB | 93% |
| Qwen3 1.7B chat | 1.58 GB | 971 MB | 1.10 GB | 92% |
| Llama 3.2 3B chat | 2.23 GB | 1.63 GB | 1.97 GB | 91% |
| Qwen3 4B chat | 2.59 GB | 1.93 GB | 2.42 GB | 90% |
| Speech (TTS + 3 STT models) | 2.34 GB | 1.71 GB | 1.21 GB | 97% |
| 3 minutes of everything at once | 2.60 GB | 2.00 GB | 1.10 GB | 98% |

One request uses the whole vCPU. The website answered every check during the whole run.

## Chat models on the CPU (1 thread)

`cluaiz bench`, prompt 512, generation 128, 2 runs; chat through the server, 3 requests of up to 64 tokens.

| Model (Q4_K_M unless named) | File | Planner RAM | Prompt t/s | Generation t/s | First token, cold / warm |
|---|---|---|---|---|---|
| Qwen3 0.6B | 379 MB | 1.09 GB | 116.0 | 36.3 | 1.9 / 0.2 s |
| Llama 3.2 1B | 771 MB | 1.11 GB | 61.6 | 21.5 | 3.5 / 0.4 s |
| Gemma 3 1B | 769 MB | 1.35 GB | 60.7 | 14.5 | 2.5 / 0.4 s |
| Qwen3 1.7B | 1.1 GB | 1.55 GB | 32.9 | 14.5 | 3.6 / 0.5 s |
| Llama 3.2 3B | 1.9 GB | 2.35 GB | 18.8 | 8.8 | 9.9 / 1.1 s |
| Phi-4-mini | 2.4 GB | 2.35 GB | 17.1 | 6.4 | 10.3 / 1.0 s |
| Qwen3 4B | 2.4 GB | 2.91 GB | 12.9 | 6.2 | 13.8 / 1.3 s |
| Bonsai 4B Q1_0 | 546 MB | 1.12 GB | 6.4 | 5.8 | 4.4 / 2.3 s |
| Ternary-Bonsai-2-4B Q2_0 | 1.8 GB (with mmproj) | 1.77 GB | 5.6 | 4.3 | 6.2 / 4.0 s |
| Qwen3 8B | 4.7 GB | 5.27 GB | refused: "3.66 GB short", with the reason | - | - |

## Speech (ONNX) and embeddings

Real-time factor = processing time / audio length; below 1 is faster than real time. Word recall = share of the
spoken words found in the transcript.

| Model | 3.6 s audio | 6.6 s audio | 10.6 s audio | Word recall |
|---|---|---|---|---|
| Kokoro-82M uint8 (TTS) | 1.16 (first call) | 0.53 | 0.55 | - |
| moonshine-tiny int8 (STT) | 0.29 | 0.14 | 0.09 | 1.00 / 0.93 / 0.76 |
| moonshine-base int8 (STT) | 0.32 | 0.21 | 0.16 | 1.00 / 0.93 / 0.97 |
| whisper-base int8 (STT) | 0.65 | 0.33 | 0.24 | 1.00 / 0.79 / 1.00 |

Embeddings, embeddinggemma-300M Q8_0: 200 sentences in 12.4 s (16.1 per second), 768 dimensions.
`cluaiz ingest`: 141 bytes in 33 ms, 35 KB in 31 ms (text and chunks; no vectors without a vector model set).

## Load

Requests at once, short answers (up to 48 tokens):

| Model | 1 | 2 | 4 | 8 |
|---|---|---|---|---|
| Qwen3 0.6B, total t/s | 4.5 (included a model switch) | 27.5 | 27.2 | 27.7 |
| Qwen3 0.6B, slowest request | 3.8 s | 1.4 s | 3.5 s | 5.9 s |
| Llama 3.2 3B, total t/s | 3.3 (included a model load) | 7.0 | 5.8 | - |
| Llama 3.2 3B, slowest request | 14.4 s | 9.2 s | 18.0 s | - |

Requests wait their turn; none failed. The total speed does not grow past 2 at once on 1 vCPU.

Everything at once for 3 minutes (2 chat streams on Qwen3 0.6B, embeddings, Kokoro TTS, moonshine-tiny STT, a
Groq chat): 71 chat answers (average 5.0 s), 212 embedding calls (0.8 s), 16 TTS (11.3 s), 16 STT (11.3 s), no
local failure. All 97 Groq requests failed with Groq's free-tier limit (429 "Rate limit reached ... requests per
minute"); the server logged the reason for each.

## Cloud provider, automation and the coding agent

| Test | Result |
|---|---|
| `cluaiz provider add groq --key-env GROQ_API_KEY`, `provider models` | works; the key stays in the environment |
| Coding agent with `groq/openai/gpt-oss-20b`: write `calc.py` and `test_calc.py`, run the tests | done in 4 s; our own `python3 -m unittest` run: OK. Cluaiz used no CPU worth noting while the provider answered |
| Coding agent with local Llama 3.2 3B | did not start: "Does not fit ... 0.89 GB short" (see problem 1) |
| Flow `vps-notes`: local model step, Groq step with a JSON schema, file write | `validate` and `save` work; `flow run` ended with "tasks.result ... error sending request" after 10 s and the file was not written (see problem 2) |
| Qwen3 8B (5.27 GB needed, 3.8 GB machine) | refused before loading, with the missing amount |

## Problems found

1. **The CLI misses a busy server.** `agent` (and `run`) check for a running server with an 800 ms request
   ([`chat/remote.rs:25`](../../../../cmd/src/chat/remote.rs)). With the one vCPU busy, the check timed out, so
   the agent tried to load the model in its own process next to the server and the planner refused for memory.
2. **`cluaiz flow run` gives up on a busy server.** Each `tasks.result` request waits 5 s on the server and times
   out after 10 s on the client ([`cli/flow.rs:12`](../../../../cmd/src/cli/flow.rs)); here the model load took
   8 s and the CPU was full, the request failed and the command exited 1. The run's final state and why the file
   was not written are not known yet.
3. From run 18: Llama 3.2 3B left `<|python_tag|>` in the answer after a tool result; the coding agent reported
   success when a 3B model changed nothing.
4. From run 16: Ternary-Bonsai Q2_0 is 13-20% slower than official llama.cpp b12428 on this CPU.
5. Not Cluaiz: Groq's free tier allows a few requests per minute, so the provider part of the load test failed.
6. On a server reachable from a network, API authentication should be on (`cluaiz token create`); Cluaiz prints
   this note when a provider is added and authentication is off.

## Sizing (from these measurements)

Everything ran on this one machine (1 vCPU, 3.8 GB); smaller machines were not tested. The memory column is what
Cluaiz held at the peak of that phase, plus about 0.6 GB for the operating system.

| Use | Cluaiz memory at the peak | Machine this points to |
|---|---|---|
| API server with cloud providers, flows, agent with a provider model | under 0.1 GB | 1 vCPU, 1 GB |
| Speech (Kokoro TTS, moonshine or whisper-base STT) and embeddings | 1.7 GB anon + mapped files | 1 vCPU, 2-4 GB |
| Chat models up to 1.7B (14-36 t/s generation) | up to 1.0 GB anon | 1 vCPU, 2 GB |
| Chat models of 3-4B (6-9 t/s) with speech | 2.0 GB anon | 1 vCPU, 4 GB |
| 7-8B chat models | needs 5.3 GB by the plan | 8 GB or more |

More vCPUs were not tested here; on 1 vCPU, parallel requests share the same speed.
