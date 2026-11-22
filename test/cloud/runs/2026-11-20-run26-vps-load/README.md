# Run 26 results: conclusion, what went well, what to fix

1 vCPU, 4 GB VPS with a web app and PostgreSQL running next to the engine. Cluaiz (a build from before
`fe21984c`), llama.cpp `b12428` and Ollama 0.40.16, same models and settings. Full numbers and method:
[SUMMARY.md](SUMMARY.md). Every number below was recounted from the raw files in this folder.

## Conclusion

Cluaiz did the same chat work as llama.cpp and Ollama at the same speed, used the least memory in the
1-hour chat soak, never failed a chat request, and was the only engine that ran chat, embeddings, text to
speech and speech to text together (1,063 of 1,063 requests in one hour, web app never down). It had two
bugs when many requests arrive at once (embeddings at 2, speech at 4); both are fixed, and run 40 on two
Linux machines (4 GB and 8 GB) checked it: 0 refused requests at 1, 2 and 4 at once. Rechecks:
[run 40](../2026-11-22-run40-cluaiz-ollama-website/SUMMARY.md).

Read with one caution: a few hours in, the host capped the CPU at 40%. Cluaiz's native stage ran before
the cap and the others after it, so only results from the same CPU state are compared below.

## What went well (Cluaiz)

| # | Result | Numbers | Compared under the same CPU? |
|---|---|---|---|
| 1 | Least memory in the 1-hour chat soak, while holding twice the queue | machine peak Cluaiz 1,759 MB (32 in flight), llama.cpp 1,811, Ollama 2,055 (16 in flight) | yes |
| 2 | No failed chat request anywhere | every W1 chat step and the chat soak: all OK (1,048 / 1,048 in the soak) | yes |
| 3 | Chat, embeddings, TTS and STT together for 1 hour | 1,063 / 1,063 (698 chat, 204 embeddings, 81 TTS, 80 STT), machine peak 2,434 MB of 3,915 | no competitor row: llama.cpp and Ollama have no speech route |
| 4 | The web app next to it kept working | 3,492 of 3,492 checks in the mixed soak; 3,559 of 3,560 in the chat soak | yes |
| 5 | Chat speed level with llama.cpp and Ollama | Qwen3 0.6B generation: full CPU 35.9 / 35.8 / 35.3 t/s; capped 12.7 / 12.2 / 13.9; soak 6.0 / 5.6 / 5.7 | yes |
| 6 | JSON answers always valid | valid JSON rate 1.0 in every W1 step | yes |
| 7 | Long tasks finished faster at full CPU | 1,000-word summary (W2), Qwen3 0.6B: 45.9 s against 118-120 s | no: Cluaiz at full CPU, the others capped |
| 8 | Ollama dropped requests that Cluaiz did not | Ollama mixed load: 15 embedding timeouts (native) and 15 (Docker); W2 with Llama 3.2 3B: 0 of 2 | partly |

## What went wrong or needs a check

| # | Problem | Numbers | State | Fix or next step |
|---|---|---|---|---|
| 1 | Embeddings refused a second request at once | at 2 at once: 1,179 of 1,252 HTTP 429 (native), 994 of 1,021 (Docker) | fixed in `fe21984c`; run 38 (local) and run 40 (Linux, 4 GB and 8 GB): 0 HTTP 429 at 1, 2, 4 at once | done |
| 2 | Speech refused requests at 4 at once | 101 of 304 HTTP 429 `speech_busy` "no permits available" | fixed in `fe21984c` (speech requests wait in line too); run 40: 0 HTTP 429 at 1, 2, 4 at once on both machines | done |
| 3 | Streamed answers had no usage chunk, so tokens could not be counted | `in_tokens` empty in every Cluaiz row | on `main` since `81ebe35f`; run 40 rows carry `in_tokens` and `out_tokens` | done |
| 4 | Docker, smallest model: Cluaiz a little slower per request | Qwen3 0.6B, one at a time: Cluaiz 1.89 s, Ollama 1.61, llama.cpp 1.67; time to first token Cluaiz 1.08 s, Ollama 0.81. Generation speed itself equal (12.7 / 12.8 t/s) | mostly fixed: about 60 ms per request went to building the flow catalog (`ef6a7113`, now 2 ms); run 40, native, full CPU: first token 0.343 s against llama.cpp 0.315 s; Docker not rerun | rerun Docker |
| 5 | Docker, Qwen3 1.7B: Cluaiz's queue passed 60 s earlier | 95th-percentile wait over 60 s at 16 at once for Cluaiz, at 32 for Ollama and llama.cpp | run 40 (native, 2 vCPU): with 4 at once Ollama was faster on 8 GB (0.6B about 15%, 1.7B about 12%), level on 4 GB 1.7B | find why with the same one slot |
| 6 | llama.cpp's Docker embeddings were faster | one request: llama.cpp Docker 4.96 chunks/s; Cluaiz Docker 1.80, Ollama Docker 1.76 | unclear: 4.96 matches the full-CPU numbers (Cluaiz native 4.84), so the cap had probably lifted by then | rerun all three in the same hour |
| 7 | One neighbour-app check timed out during the chat soak | 1 of 3,560 (5 s timeout at 2026-11-21 04:15:45 UTC) | run 40: 0 failed of 88,212 and 90,111 website requests | done |

## What went wrong with the test itself

| Problem | Effect | Next time |
|---|---|---|
| The host capped the CPU at 40% after Cluaiz's native stage | native speed and "break" levels compare full CPU with capped CPU; an earlier summary wrongly claimed 2.6x faster | interleave engines in short blocks; record steal time and the provider's cap |
| The Cluaiz commit was not recorded | the fixed bugs cannot be tied to a build | record `git rev-parse HEAD` with the binary |
| Answer texts were not saved | answer length (Cluaiz 6-10 chunks, the others 14-15 tokens) cannot be checked | save every answer |
| Old Cluaiz build | problems 1-3 were already fixed in code | build from current `main` |

## Before a public post

1. Done in run 40: W1, W4 and W5 rerun with the current build, engines interleaved, answer texts saved
   (Cluaiz and Ollama; llama.cpp did not start on the fresh VMs).
2. Still open: llama.cpp in the same run, Docker, and why Ollama is faster with 4 requests at once.
3. Post from the rows that hold in run 40: no failed request, about 30% less memory than Ollama, the
   website untouched, no refused embedding or speech request.
