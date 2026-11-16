# Run 18: features on the 1 vCPU VPS without a GPU (2026-11-16)

| | |
|---|---|
| Machine | 1 vCPU (AMD EPYC 9354P), 3.8 GB RAM, no GPU, Ubuntu 24.04; a website container runs beside it |
| Cluaiz | v0.1.0 re-published release (tag `eb11d47d`), server on 127.0.0.1:8011 in a 2.9 GB memory scope |
| Script | [`vps_run18_features.sh`](../../remote/vps_run18_features.sh), checks from [`features11.py`](../../remote/features11.py) |

## Results: 16 of 17 feature checks passed

| Check | Model | Result | Seconds |
|---|---|---|---|
| Streamed chat, cold (model load) | Llama 3.2 3B Q4_K_M | first token 10.4 s | 13.8 |
| Streamed chat, warm | Llama 3.2 3B Q4_K_M | first token 0.12 s | 3.4 |
| Tool call | Llama 3.2 3B / Bonsai 4B / Ternary-Bonsai-2-4B | pass / pass / pass | 10.2 / 26.9 / 56.8 |
| Answer after the tool result | Llama 3.2 3B | **fail**: the answer was `<|python_tag|>` | 3.8 |
| Answer after the tool result | Bonsai 4B / Ternary-Bonsai-2-4B | pass / pass | 13.6 / 9.4 |
| Thinking kept apart from the answer | all three | pass (Ternary-Bonsai: 848 characters of reasoning) | 2.4 / 10.2 / 67.1 |
| Second turn reuses the cache | all three | pass (183-211 tokens reused, 15-25 new) | 11.6 / 29.3 / 38.8 |
| Vision (blue left, yellow right) | Ternary-Bonsai-2-4B + its mmproj | pass | 16.5 |
| Embeddings | embeddinggemma-300M Q8_0 | pass, 768 dimensions, similar 0.93 / different 0.36 | 0.5 |
| TTS | Kokoro-82M ONNX uint8 | pass, 3.6 s of audio | 7.4 |
| STT | moonshine-tiny ONNX int8 | pass, exact text | 0.8 |
| STT streamed | moonshine-tiny ONNX int8 | pass, 11 chunks, first after 0.08 s | 0.15 |

Other parts:

| Part | Result |
|---|---|
| `cluaiz status`, `list`, `show` | work; the planner gives every model "CPU only" |
| Pulls | Kokoro 24 s, moonshine-tiny 7 s, embeddinggemma 7 s |
| `cluaiz ingest` of a small Markdown file | 4 chunks in 0.02 s; no vectors, because no embedding model is set as the vector model |
| `cluaiz mcp-server` over stdio | `initialize` answered, `tools/list` gave 52 tools |
| Coding agent, Llama 3.2 3B, fix `add.py` | **fail**: exit 0 after 2 min 39 s, but no file changed; the model wrote its edit call as JSON text and inside its thinking, which the agent did not run |
| Memory at the end | 2.8 GB used of 3.8 GB, 1.1 GB available; the website kept answering |

## Problems found

1. Llama 3.2 3B: after a tool result, `<|python_tag|>` reached the answer. The model starts another tool
   call in its own format, and the parser leaves the marker in the text.
2. The coding agent reports success (`exit_code` 0, `is_error` false) when the model made no change and
   called no tool. A 3B model is weak for the agent, but the result should say that nothing was done.
3. Kokoro TTS on 1 vCPU: 7.4 s for 3.6 s of audio (about 2x slower than real time). Run 19 measures it in
   more detail.
