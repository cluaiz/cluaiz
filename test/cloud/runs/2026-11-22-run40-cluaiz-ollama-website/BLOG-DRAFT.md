<!-- Draft for cluaiz.com. Not published. Every number links to a run folder in this repo; check them
again before publishing, and replace relative links with GitHub URLs. -->

# Running AI next to your app on a small VPS: what we measured

Many developers already pay for a small Linux server that runs their website and its database. Adding
an AI feature to it (classifying support tickets, summarising, search over documents, a voice reply) is
the obvious next step. It is also where things tend to break: the model fits in a terminal test, then
the app gets errors, or the database slows down, or a second request makes everything wait.

We ran Cluaiz, llama.cpp and Ollama on 4 GB and 8 GB Linux servers with a real website and 20 simulated
users beside them, and on a 1 vCPU, 3.8 GB VPS. This post is what we learned, including where Cluaiz is
behind.

## Why put the AI on the same server

- **You already pay for it.** A hosted model API is a new bill per token. A model on your own server
  costs nothing per request.
- **The data stays on your machine.** Ticket text, documents and recordings never leave it.
- **One less service to depend on.** No API key that can be rate limited or revoked.

The cost is speed and model size: a 2 vCPU server is not a GPU, so think small models (0.6B to 3B
parameters) and short jobs.

## What usually goes wrong

From public reports we collected (sources in our competitor study, `demand/gpt-small-vps.md`):

- A team running PostgreSQL, Node.js, React and Ollama on a 4 GB, 1 vCPU VPS reported disabling
  database connections to run the AI, then moved to 8 GB.
- A 4 GB trial of Dify with Ollama took about a minute per chat reply.
- Model runners, document indexers and workflow tools each can run the machine out of memory, and two
  jobs at once can time out while memory still looks fine.

So the questions are: does it fit, does the app stay healthy, and what happens when requests overlap.

## What we measured

Full numbers, method and raw files: [README.md](README.md) and [SUMMARY.md](SUMMARY.md).

**The website stayed healthy.** On both Google Cloud machines (2 vCPU, 4 GB and 8 GB) a Node.js shop with
PostgreSQL served 88,212 and 90,111 requests from 20 simulated users while the engines worked, with no
failure; 95% of pages answered within 11 and 16 ms.

**No chat request failed**, on any engine, in any step.

**Memory.** For the same model and settings, Cluaiz used 534-560 MB against Ollama's 782-788 MB on
Qwen3 0.6B, and 1,004 MB against 1,448 MB on Qwen3 1.7B, about 30% less. In a one-hour soak on the
3.8 GB VPS, Cluaiz had the lowest machine memory of the three (1,759 MB, with twice the queue).

**Speed, one request at a time:** level. Qwen3 0.6B took 1.32-1.35 s per request on Cluaiz and
1.31-1.32 s on Ollama; on the VPS Cluaiz 0.625 s and llama.cpp 0.629 s. On Qwen3 1.7B Cluaiz finished
sooner (2.8 s against 3.7 s), partly because its answers were shorter.

**Speed, four requests at once:** Ollama was faster, by about 15% on 0.6B and 12% on 1.7B. We have not
found why yet.

**Chat, embeddings and speech in one program.** On the VPS, Cluaiz ran chat, embeddings, text to speech
and speech to text together for an hour: 1,063 of 1,063 requests, the web app never down. On the Google
machines, 1, 2 and 4 embedding or speech requests at once all completed; none was refused.

**Pick the model for the job.** Qwen3 0.6B chose the right ticket label about 3 times in 10 on every
engine; Qwen3 1.7B about 8 times in 10. The engine does not change that. If the answer matters, start at
1.7B.

## One trap: "burst" VPS plans

Our first VPS test ran on a plan with CPU credits. After about three hours at full load the host
limited the CPU to 40%, halfway through the test, and the engines that ran later looked slower. If your
server is a burst plan, a benchmark that runs engines one after another is not a fair comparison, and a
long batch job will slow down after a few hours. Dedicated vCPU plans do not do this.

## How to set it up with Cluaiz

1. Install (Linux and macOS):

   ```bash
   curl -fsSL https://cluaiz.com/install.sh | bash
   ```

2. Pull a model, for example Qwen3 1.7B:

   ```bash
   cluaiz pull unsloth/Qwen3-1.7B-GGUF:Q4_K_M
   ```

3. Decide who can reach the API. The server listens on every address by default (`api_host` is
   `0.0.0.0`) and asks for no token. On a VPS, choose one:
   - your app runs on the same server: start with `CLUAIZ_HOST=127.0.0.1 cluaiz serve`, so only local
     programs reach it;
   - other machines must reach it: `cluaiz token create` turns on API keys, and keep port 8000 closed in
     the firewall except for the machines that need it.

4. Start it and call it like OpenAI's API:

   ```bash
   cluaiz serve
   curl http://127.0.0.1:8000/v1/chat/completions -H "Content-Type: application/json" \
     -d '{"model": "<id from cluaiz list>", "messages": [{"role": "user", "content": "Hello"}]}'
   ```

   Code written for Ollama's API can point at Cluaiz too: its `/api/chat`, `/api/generate`,
   `/api/embed` and `/api/tags` routes answer in Ollama's shape (see "Coming from Ollama" in the docs).

## When to pick something else

- **You only need one model and Ollama or llama.cpp already works for you.** Keep it. They are mature,
  and Ollama was faster with several requests at once in our test.
- **You need many users, teams or image generation.** Open WebUI, AnythingLLM or LocalAI cover that;
  Cluaiz has no multi-user or image generation.
- **The model is too slow for the job.** A hosted API, or a bigger server: on DigitalOcean's Basic plans
  (read 2026-11-20) 4 GiB was $24 and 8 GiB $48 a month.

## Why we think Cluaiz fits this case

Not because any single feature is unique: Ollama and llama.cpp have speech routes now, and other
products have memory, tools or agents. What a small server rewards is fewer programs. Cluaiz runs the
models, embeddings, speech, memory, tools and an agent in one process with one permission system, and
on the current code the agent's commands run in an OS sandbox on Linux and macOS. On our 4 GB test that
meant less memory than Ollama and an app that did not notice the AI was there.

What is not done: the OS sandbox is on `main`, not in the 0.1.0 release; we have not measured llama.cpp
on the Google machines, Docker since run 26, or Apple hardware; and Cluaiz is slower than Ollama with
several requests at once. The test scripts are in the repository if you want to check our numbers on
your own server.
