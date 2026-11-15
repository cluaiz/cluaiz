# Run 11: v0.1.0 after the llama.cpp update, new models, three tools compared (2026-11-15)

The republished v0.1.0 (`51d03856`: llama.cpp fork `17d1465f0`, the smaller VRAM reserve when no other
process uses the GPU, the RAM reserve capped at 4 GB) on the models the earlier cloud runs did not use.
Run 11 found three bugs. They were fixed in `48bd4d60` and `c3b1b123`, and v0.1.0 was published again.
[Run 12](../2026-11-15-run12-l4/SUMMARY.md) tested the fixes again.

| | |
|---|---|
| VM | GCP `g2-standard-8`: NVIDIA L4 22.5 GB, driver 620.70, Xeon 2.2 GHz 4 cores / 8 threads, 31 GB RAM, Debian 13 |
| Cluaiz | release v0.1.0 by `install.sh`, isolated home, port 8011 |
| Reference | `llama-bench` built from the same llama.cpp source (`17d1465f0`) |
| Other tools | Ollama 0.40.16 (`ollama create` from the same GGUF), LM Studio CLI (`lms import --symbolic-link`, `--gpu max`) |
| Scripts | [`run11.sh`](../../remote/run11.sh), [`features11.py`](../../remote/features11.py), [`compare3.py`](../../remote/compare3.py), [`ssd_test.sh`](../../remote/ssd_test.sh) |

## Placements against llama.cpp (prompt / generation t/s, warm runs averaged)

| Model | auto | gpu | split | cpu |
|---|---|---|---|---|
| Phi-4-mini Q4_K_M | 5801 / 88.5 vs 5588 / 88.9 | 5881 / 88.6 vs 5541 / 89.0 | 869 / 14.8 vs 717 / 15.1 | 42 / 6.6 vs 42 / 7.9 |
| Qwen3.5-9B Q4_K_M | 2326 / 43.7 vs 2478 / 43.7 | 2354 / 43.8 vs 2496 / 43.9 | 466 / 9.5 vs 393 / 9.7 | 27 / 4.9 vs 27 / 5.1 |
| Qwen3-30B-A3B-2507 Q4_K_M | 2694 / 98.9 vs 2706 / 99.4 | 2696 / 98.3 vs 2686 / 99.2 | 193 / 17.9 vs 236 / 16.9 | 32 / 7.9 vs 34 / 9.9 |
| gpt-oss-20b MXFP4 | FAIL (control tokens) | FAIL | FAIL | FAIL |

Each cell is Cluaiz vs llama-bench. Reports: `results-regression/RESULTS_l4r11_*.md`.

- On the GPU, generation is within 1% of llama.cpp. Prompt speed is within ±6%, the same spread as in
  run 9. The engine measures one real request, while llama-bench averages several runs from an empty context.
- **CPU only:** generation is 0.80x (Qwen3-30B), 0.84x (Phi-4) and 0.96x (Qwen3.5-9B) of llama-bench. This is
  the first CPU-only comparison, so there is no earlier number to call it a regression. llama-bench does not
  sample, and the engine samples every token on the same 4 cores. Not explained yet: a comparison with
  llama-server is still to run.
- gpt-oss-20b answered at the same speed (gpu 81.5 t/s), but every answer carried
  `<|start|>assistant<|channel|>final<|message|>`. Fixed in `c3b1b123` (see below).

## Features (22 / 24)

| Check | Result |
|---|---|
| Tool call (4 models) | 4 / 4 |
| Answer after the tool result | Qwen3-30B, Qwen3.5-9B pass; Phi-4-mini asked to call the tool again; gpt-oss answered with its reasoning (the check passed by mistake) |
| Thinking kept apart from the answer | 3 / 4; gpt-oss leaked control tokens |
| Second turn reuses the cache | 4 / 4 (182-246 tokens reused) |
| Vision (Qwen3.5-9B) | pass |
| Embeddings (embeddinggemma 300M) | pass, similar 0.93 / different 0.36 |
| TTS: Kokoro, Qwen3-TTS 1.7B | 2 / 2 |
| STT moonshine-tiny, plain and streaming | 4 / 4; first streamed event after 0.06-0.07 s, 11-12 chunks |

## Bugs found and fixed

1. **gpt-oss answers kept the harmony channel markup.** Only requests with tools were read by llama.cpp's
   chat parser, so a plain answer kept everything outside the thinking tags. Answers without tools are now
   parsed too, as llama-server does, when the format is not content-only and the parser's render equals the
   prompt (`c3b1b123`).
2. **Tool calls a client sent back were dropped.** An assistant message's `tool_calls` never reached the
   prompt. Lenient templates hid it. gpt-oss's template failed on the tool result ("Message has tool role,
   but there was no previous assistant message with a tool call"), and the request fell back to a plain
   render. The Phi-4-mini failure above was the same bug. Fixed in `c3b1b123`.
3. **An untagged non-interactive pull took a draft model.** `cluaiz pull ggml-org/gpt-oss-20b-GGUF` had no
   Q4_K_M, so it took the smallest file, the 0.9 GB `eagle3-...-Q8_0.gguf`. Files that llama.cpp's `-hf`
   download does not take as the model (`dflash-`, `eagle3-`, `dspark-`, `imatrix`) are no longer variants
   (`48bd4d60`).

## SSD streaming: gpt-oss-120b MXFP4 (59 GiB) on 31 GB RAM + 22.5 GB VRAM

| | Prompt t/s | Generation t/s |
|---|---|---|
| Cluaiz, request 1 (cold, from disk) | 4.4 | 3.9 (531 s for the request) |
| Cluaiz, request 2 (warm) | 25.3 | 8.2 |
| llama-bench, same placement | 33.6 | 10.5 |

The planner chose all layers on the GPU, with the experts of 25 layers in RAM. Planned VRAM was
20.49 GB, and 20.74 GB was taken. The answer only showed the channel markup (bug 1, within 64 tokens).
The llama-bench numbers are not a fair comparison: the model is twice the RAM size, every token reads
from disk, and llama-bench ran right after Cluaiz with the page cache it left. Each side also ran once.
The comparison needs the cache dropped before each side and three runs. The script's "disk read" figure
reads 0.00 GB, which is wrong.

## Cluaiz, Ollama and LM Studio on the same files

Context 16384, prompt of about 588 tokens, 64 new tokens, thinking off, one tool at a time (the others
stopped). Each request was sent cold once after dropping the page cache, then warm three times; the
speeds are the warm average.

| Model | Tool | Cold request | Prompt t/s | Generation t/s | GPU memory |
|---|---|---|---|---|---|
| gpt-oss-20b MXFP4 | Cluaiz | 70.5 s | 2751 | **81.6** | 12.4 GB |
| | Ollama | 124.0 s | **3053** | 78.5 | 11.8 GB |
| | LM Studio | 72.1 s | 2719 | 62.2 | 11.7 GB |
| Phi-4-mini Q4_K_M | Cluaiz | **14.8 s** | 5790 | **88.6** | 5.1 GB |
| | Ollama | 30.5 s | **6026** | 85.8 | 4.8 GB |
| | LM Studio | 18.2 s | 5473 | 66.8 | 4.7 GB |
| Qwen3-30B-A3B-2507 Q4_K_M | Cluaiz | **103.1 s** | **2723** | **99.1** | 19.7 GB |
| | Ollama | 116.7 s | 1874 | 97.2 | 19.4 GB |
| | LM Studio | 105.6 s | 2185 | 77.5 | 19.4 GB |

- Generation: Cluaiz is the fastest on all three models, 2-4% ahead of Ollama and 28-33% ahead of
  LM Studio. LM Studio loads with 4 parallel slots by default.
- Cold request: Cluaiz is the fastest on all three.
- Prompt: Ollama is ahead on gpt-oss (+11%) and Phi-4 (+4%), and Cluaiz is ahead on Qwen3-30B (+45%).
  LM Studio's prompt figure comes from its time to the first token.
- Cluaiz uses 0.2-0.6 GB more GPU memory.
- The gpt-oss rows were measured before fix 1 (the Cluaiz reply carried the markup). Run 12 repeats the
  Cluaiz row.

## Gemma 4 31B Q4_K_M with the new reserve (auto placement)

Full GPU, 12.6-12.8 t/s generation, 20.2 of 23.0 GB used. In run 10 the planner kept 3 of 60 blocks in RAM
(8.9 t/s), while Ollama ran it fully on the GPU at 12.8 t/s. The planner saw no other process on the GPU,
so it kept a 0.15 GB reserve, and the full GPU now fits. The context came out at 4096 tokens: the GPU pool
was measured at 2.16 GB and llama.cpp's dry run added 1.09 GB, which left little room for the KV cache.

## Test-side problems in this run (not engine bugs)

- The `~/ch` model folder was deleted on the VM without checking that `~/ch-reg` linked to it; the earlier
  test models were downloaded again where needed.
- A leftover placement run and the features run shared port 8011 for a few minutes; those results were
  removed and both steps ran again.
- LM Studio's installer needs `ldconfig` on `PATH` (`/sbin` on Debian). `compare3.py` now finds the
  imported model by its file name.

## Not done

- CPU-only gap: a comparison with llama-server, which also samples every token.
- SSD streaming: a comparison with the cache dropped before each side and three runs.
- Gemma 4 31B: why the context is 4096 when the model is fully on the GPU.
- `max_tokens` does not count thinking tokens: Qwen3.5-4B thought for 5000+ tokens with `max_tokens` 500, on
  v0.1.0 before and after the fixes.
