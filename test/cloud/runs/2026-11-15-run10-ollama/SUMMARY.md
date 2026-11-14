# Run 10: Cluaiz and Ollama on the same machines (2026-11-15)

Both tools as a user gets them, each with its own defaults, on the same VM, the same GGUF files and the
same prompt. Script: [`ollama_compare.py`](ollama_compare.py).

| | |
|---|---|
| Cluaiz | release v0.1.0 (`14ae569f`) by `install.sh`, chat API `/v1/chat/completions` |
| Ollama | 0.40.16 by its `install.sh`, chat API `/api/chat`; each model imported with `ollama create` from the file Cluaiz downloaded (same bytes) |
| GPU VM | `g2-standard-8`, NVIDIA L4 23 GB, driver 620.70, Debian 13, context 16384 for both |
| CPU VM | `n2-standard-8`, Xeon 2.8 GHz 8 vCPU, 31 GB, Ubuntu 24.04, context 4096 for both |
| Request | run.py's prompt (random number + ~535 tokens), 64 new tokens, thinking off; 1 cold request (includes loading the model) + 3 warm |
| Order | one tool at a time; the other is stopped, so the whole GPU is free |

Raw rows: `l4/ollama-l4/raw.jsonl`, `cpu/ollama-cpu/raw.jsonl` (also the `ollama ps` lines); Cluaiz server logs beside them.

## NVIDIA L4 (warm requests averaged)

| Model (Q4_K_M) | Tool | Placement | Prompt t/s | Generation t/s | GPU memory | Cold request |
|---|---|---|---|---|---|---|
| Qwen3 4B | Cluaiz | full GPU | 4541 | 82.9 | 5.1 GB | 15.3 s |
| | Ollama | 100% GPU | 5437 | 81.9 | 5.0 GB | 89.6 s (load 47.8 s) |
| Gemma 3 12B | Cluaiz | full GPU | 1485 | 30.4 | 13.6 GB | 42.6 s |
| | Ollama | 100% GPU | 1430 | 30.6 | 8.9 GB | 49.9 s |
| Mistral Small 3.2 24B | Cluaiz | full GPU | 910 (527 tokens) | 17.1 | 16.0 GB | 82.9 s |
| | Ollama | 100% GPU | 1964 (1085 tokens) | 17.4 | 16.0 GB | 85.9 s |
| Gemma 4 31B (18.7 GB) | Cluaiz | GPU + RAM (planner) | 439 | 8.9 | 20.4 GB | 109.0 s |
| | Ollama | 100% GPU | 530 | **12.8** | 19.9 GB | 159.6 s |

## CPU only (warm requests averaged)

| Model (Q4_K_M) | Tool | Prompt t/s | Generation t/s | Cold request |
|---|---|---|---|---|
| Qwen3 4B | Cluaiz | 57.2 | 10.4 | 32.4 s |
| | Ollama | 57.3 | 10.2 | 17.8 s |
| Gemma 3 12B | Cluaiz | 19.9 | 4.2 | 84.3 s |
| | Ollama | 20.2 | 4.3 | 44.0 s |

## What it shows

1. **Generation speed is the same** whenever both put the model in the same place (within 2%, GPU and CPU):
   both run llama.cpp underneath.
2. **Gemma 4 31B:** Ollama puts the whole model on the L4 and generates 12.8 t/s; Cluaiz's planner keeps
   3 of 60 blocks in RAM and generates 8.9 t/s (the same gap as run 9). Cluaiz needs a planner fix here.
3. **Gemma 3 12B uses 4.7 GB more GPU memory in Cluaiz** at the same context (13.6 vs 8.9 GB).
4. **Cold request on the CPU:** Ollama loads and answers the first request about twice as fast (18 vs 32 s,
   44 vs 84 s). On the GPU Cluaiz's first request was as fast or faster (Ollama's first Qwen3 load after the
   import took 48 s).
5. **Mistral prompt numbers are not comparable:** each Ollama request processed 1085 prompt tokens (the
   model's long system prompt every time); Cluaiz processed 1085 on the cold request and 527 afterwards
   (the system prompt stays cached). Prompt t/s grows with the batch, so the two rows measure different work.
6. Qwen3 4B prompt: Ollama 5437 vs Cluaiz 4541 t/s on the L4 (Ollama ~20% faster); equal on the CPU.

Not compared here: speech, embeddings and Ollama's own model library builds; only the shared GGUF chat path.
