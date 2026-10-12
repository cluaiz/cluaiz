# Cluaiz vs llama.cpp: test results (2026-10-12)

## Who is who in this file

| Name in the tables | What it is | How it was run |
|---|---|---|
| **Cluaiz** | Our engine: `target/release/cluaiz.exe serve`, called through its chat API (`/v1/chat/completions`) | Planner chooses the placement, prompt is split evenly, tokens are sampled and streamed like a real chat |
| **llama.cpp** | Plain llama.cpp, nothing from Cluaiz: its own `llama-bench` tool | Built from the **same** llama.cpp source as Cluaiz, with CUDA, given the **same** placement Cluaiz chose |

Both run on the same laptop, the same model file and the same placement, so the only difference is Cluaiz's own
code around llama.cpp. Speeds are in tokens per second (t/s): higher is better.

## Short answer

| | Cluaiz vs llama.cpp |
|---|---|
| Generation speed (writing the answer) | **Same** in all 18 tests (within ±5%, the normal run-to-run noise) |
| Prompt speed, model fully on GPU | **Same or Cluaiz slightly faster** |
| Prompt speed, model split GPU + RAM | **Cluaiz 1.4x to 2.2x faster** |
| Gemma 4 26B MoE (bigger than the GPU) | **Cluaiz runs it** (7.4 to 18 t/s, no out-of-memory); llama.cpp: ~4-4.4 t/s or out of memory (user-reported) |
| Tests passed | 18 of 18 |

## Machine

| | |
|---|---|
| CPU | AMD Ryzen 7 7435HS, 8 cores / 16 threads, AVX2 |
| RAM | 23.7 GB (free RAM was between ~3 and ~8 GB during the night) |
| GPU | NVIDIA GeForce RTX 3050 A Laptop, 4 GB VRAM (~2.9 GB free when idle) |
| Disk | NVMe SSD |
| OS | Windows 11 (build 26300), CUDA 13.2 |
| llama.cpp source | cluaiz fork `cluaiz-main` (commit 44cf982), used by both Cluaiz and the llama.cpp reference |

## Generation speed (t/s): Cluaiz vs llama.cpp

| Model (file size) | Mode | Placement | **Cluaiz** | **llama.cpp** | Result |
|---|---|---|---|---|---|
| Llama 3.2 3B Q4_K_M (1.88 GB) | auto | full GPU | 51.7 | 51.9 | same |
| | gpu | full GPU | 51.9 | 51.8 | same |
| | split | 14 of 28 blocks on GPU | 23.4 | 24.9 | same (-6%) |
| Qwen3 4B Q4_K_M (2.33 GB) | auto | full GPU | 39.3 | 38.7 | same |
| | gpu | full GPU | 39.2 | 39.4 | same |
| | split | 18 of 36 blocks on GPU | 15.9 | 14.9 | same (+7%) |
| Gemma 4 E4B Q4_K_M (4.64 GB) | auto | 25 of 42 blocks on GPU | 15.5 | 16.4 | same (-5%) |
| | gpu | full GPU | 32.3 | 32.0 | same |
| | split | 21 of 42 blocks on GPU | 14.5 | 14.6 | same |
| Bonsai 8B (1.08 GB) | auto | full GPU | 72.7 | 71.9 | same |
| | gpu | full GPU | 72.3 | 71.2 | same |
| | split | 18 of 36 blocks on GPU | 17.7 | 18.1 | same |
| Bonsai 4B Q1_0 (0.53 GB) | auto | full GPU | 111.0 | 106.3 | same (+4%) |
| | gpu | full GPU | 108.2 | 112.3 | same (-4%) |
| | split | 18 of 36 blocks on GPU | 27.6 | 28.2 | same |
| Bonsai 27B Q1_0 (3.54 GB) | auto | 24 of 64 blocks on GPU | 4.0 | 3.7 | same (+8%) |
| | gpu | full GPU (spills into shared GPU memory) | 8.1 | 7.8 | same |
| | split | 32 of 64 blocks on GPU | 5.2 | 4.8 | same (+8%) |

## Prompt speed (t/s): Cluaiz vs llama.cpp

Prompt of about 530 tokens.

| Model | Mode | **Cluaiz** | **llama.cpp** | Result |
|---|---|---|---|---|
| Llama 3.2 3B | auto (full GPU) | 2578 | 2446 | Cluaiz +5% |
| | gpu (full GPU) | 2579 | 2473 | Cluaiz +4% |
| | split | 1111 | 810 | **Cluaiz 1.4x** |
| Qwen3 4B | auto (full GPU) | 1863 | 1744 | Cluaiz +7% |
| | gpu (full GPU) | 1846 | 1801 | same |
| | split | 810 | 529 | **Cluaiz 1.5x** |
| Gemma 4 E4B | auto (split) | 789 | 568 | **Cluaiz 1.4x** |
| | gpu (full GPU) | 1666 | 1605 | same |
| | split | 705 | 452 | **Cluaiz 1.6x** |
| Bonsai 8B | auto (full GPU) | 1352 | 1334 | same |
| | gpu (full GPU) | 1352 | 1319 | same |
| | split | 769 | 385 | **Cluaiz 2.0x** |
| Bonsai 4B | auto (full GPU) | 2270 | 2129 | Cluaiz +7% |
| | gpu (full GPU) | 2239 | 2240 | same |
| | split | 1186 | 692 | **Cluaiz 1.7x** |
| Bonsai 27B | auto (split) | 206 | 95 | **Cluaiz 2.2x** |
| | gpu (shared memory) | 47 | 30 | **Cluaiz 1.6x** |
| | split | 230 | 110 | **Cluaiz 2.1x** |

**Why Cluaiz is faster on the prompt when the model is split:** llama.cpp sends the prompt in chunks of 512
tokens. A 530-token prompt becomes 512 + 18. For the layers kept in RAM, llama.cpp computes a chunk smaller than
32 tokens on the CPU, which is slow. Cluaiz splits the same prompt into 265 + 265, so both chunks take the fast path.
On full GPU there is no RAM part, so both are about the same.

## Gemma 4 26B A4B MoE (13.27 GB file, 4 GB GPU)

The model does not fit in the GPU and barely fits in free RAM. Cluaiz keeps attention on the GPU, keeps the
experts in RAM and lets Windows read the rest from the SSD when needed.

| Who | Situation | Prompt | Generation |
|---|---|---|---|
| **Cluaiz** | ~8 GB free RAM, warm | ~171 t/s | 16-18 t/s |
| **Cluaiz** | first message right after loading (experts still on SSD) | ~34 t/s | ~15 t/s |
| **Cluaiz** | inside the app with only ~3 GB free RAM | 8.9 t/s | 7.4 t/s |
| **Cluaiz** | forced `n_gpu_layers -1` (all 13 GB on a 4 GB GPU) | does not load (correct: it cannot fit) | - |
| **llama.cpp** | user-reported, not measured in this test | - | ~4-4.4 t/s or out of memory |

- Cluaiz placement chosen by the planner: attention + KV (F16, n_ctx 4096) on GPU, experts of all 30 layers in
  RAM (`n_gpu_layers -1`, `n_cpu_moe 30`), op_offload on.
- Turning op_offload off made the prompt slower (~46 t/s), so the llama.cpp default (on) was kept.
- Speed depends on free RAM: the less free RAM, the more experts are read from the SSD on every token.

## Time to first token (Cluaiz only)

Bonsai 8B, full GPU, short message, model already loaded.

| Cluaiz version | Time to first token | Cluaiz work before llama.cpp starts |
|---|---|---|
| Before this night's fixes | 0.16-0.31 s | ~160 ms (chat template parsed 4 times per message) |
| After the fixes | **0.09-0.10 s** | 37-43 ms (template parsed once per model) |

llama.cpp itself needs ~33 ms for this prompt; the rest is Cluaiz's API work (model registry ~20 ms, tools
~10 ms, template ~12 ms). Model loading is reported separately (`model_load_ms`) and is not counted in it.

## Correctness checks (Cluaiz)

| Check | Result |
|---|---|
| BOS token | Llama 3 prompt starts with token 128000 and Gemma with token 2, like llama.cpp; Qwen has no BOS, unchanged |
| Thinking | Qwen3 `<think>` and Gemma 4 `<|channel>thought ... <channel|>` go to `reasoning_content`, the answer to `content`; no tag text leaks (streaming and non-streaming) |
| Thinking off | no reasoning returned |
| Leaked control tokens (`<|...|>`) | 0 in all 18 tests |
| Wrong `llama` request params | typo or unsupported value is rejected with a clear error |
| Settings update from the app | only the changed setting is saved, the others stay; bad value returns 422 with the reason |

## Settings used

**Cluaiz** (defaults, the same for every test unless the mode changes placement):

| Setting | Value |
|---|---|
| n_ctx | Auto, at least 4096 (planner) |
| KV cache | Auto: F16, or Q8_0 when F16 does not fit (planner; the chosen type was not recorded per test) |
| flash attention | Auto (llama.cpp turns it on for CUDA) |
| op_offload | on |
| threads | 8 (physical cores) |
| batch / ubatch | 512 / 512 |
| mmap | on |
| VRAM kept free | 1024 MiB, else 512 MiB |
| prompt chunks | equal size, at most 512 tokens |

**Modes** (what was sent in the request's `llama` params):

| Mode | Sent |
|---|---|
| auto | nothing: the Cluaiz planner decides |
| gpu | `n_gpu_layers: -1` (every layer on the GPU) |
| split | half of the blocks on the GPU |

**llama.cpp** (`llama-bench`): the same `-ngl` / `-ncmoe` / KV type Cluaiz used in that test, `-fa 1`,
8 threads, `-b 2048 -ub 512`, `-r 2`. Prompt: `-p <tokens> -n 0`. Generation: `-p 0 -n 64 -d <tokens>`
(after the same number of context tokens as the Cluaiz run).

**Requests to Cluaiz:** prompt ~530 tokens with a random number in front (no cache reuse), `max_tokens` 64,
3 runs per test. Run 1 is the cold run (also checks thinking and leaks); the speeds are the average of runs 2-3
with thinking off, so exactly 64 tokens are generated.

## Not covered by this test

- Only this laptop: no other GPU (AMD, Intel, bigger NVIDIA), no Mac or Linux.
- CPU-only mode and long chats (8k+ tokens).
- Gemma 4 26B is not in the 18-test tables; its llama.cpp number is user-reported.
- Thinking tokens are not limited by `max_tokens` (only by `reasoning_effort`).
- Laptop GPU speed changes a few percent between runs (clock, temperature).

## Run it again

```
cargo build --release -p cmd
target/release/cluaiz.exe serve
python test/engine_regression/run.py --modes auto,gpu,split --runs 3 \
    --llama-bench target/llama-ref/bin/Release/llama-bench.exe --report test/engine_regression/report.md
```

`--report` writes a new file with every setting per test (KV type, n_ctx, threads, batch, flash attention, mmap)
and the exact llama-bench command.
