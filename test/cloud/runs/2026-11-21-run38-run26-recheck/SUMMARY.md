# Run 38: the two Cluaiz problems from run 26, checked on the current code (2026-11-21)

Windows 11 (build 26300), Engine 0.1.0 debug build of `main` at `10e9a8fb`, isolated `CLUAIZ_HOME` on port
8011, Qwen3-0.6B Q4_K_M and EmbeddingGemma 300M Q8_0 (`ggml-org/embeddinggemma-300M-GGUF:Q8_0`).
Script: `probe.py`; output: `results.txt`.

| Check | Run 26 (older build, VPS) | Run 38 (current code) |
|---|---|---|
| `/v1/embeddings`, 8 texts, 2 requests at once | 8,160 of 8,163 HTTP 429 `embedding_busy` | 2 of 2 HTTP 200 (0.06 s, 0.11 s) |
| same, 4 requests at once | not run | 4 of 4 HTTP 200, answered in turn (0.06 s to 0.21 s) |
| Streamed chat with `stream_options.include_usage` | no usage chunk; the harness counted 10 chunks as tokens | usage chunk sent; `completion_tokens` 33 = 33 content chunks, 3 of 3 requests |

The queue is `fe21984c` (requests wait for the running one, up to 120 s, then HTTP 429). The usage chunk
is `81ebe35f`.

## Not proven

- This machine has a GPU; the VPS numbers of run 26 were not rerun with the current build.
- Whether Cluaiz's answers in run 26 had 10 or 15 tokens: the answer texts were not saved.
