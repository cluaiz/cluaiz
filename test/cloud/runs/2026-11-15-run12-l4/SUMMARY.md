# Run 12: the fixes of run 11, again on the L4 (2026-11-15)

v0.1.0 was published again from `c3b1b123`. This run repeats only what the three fixes touch; the rest of
[run 11](../2026-11-15-run11-l4/SUMMARY.md) stands. The VM and isolated home are the same as in run 11,
and the GPU driver packs are unchanged. Script: [`run12.sh`](../../remote/run12.sh).

| Check | Run 11 | Run 12 |
|---|---|---|
| `cluaiz pull ggml-org/gpt-oss-20b-GGUF` without a tag (non-interactive) | took `eagle3-gpt-oss-20b-Q8_0.gguf` (0.9 GB) | took `gpt-oss-20b-MXFP4.gguf` (12.1 GB) |
| gpt-oss-20b placements | 0 / 4 (control tokens in every answer) | **4 / 4** |
| Qwen3.5-9B auto, gpu | 2 / 2 | 2 / 2 |
| Features | 22 / 24 | **24 / 24** |

## gpt-oss-20b against llama.cpp (prompt / generation t/s)

| Mode | Cluaiz | llama-bench |
|---|---|---|
| auto | 2604 / 81.8 | 2923 / 81.9 |
| gpu | 2790 / 81.8 | 2923 / 81.6 |
| split | 262 / 16.0 | 305 / 16.4 |
| cpu | 21 / 8.8 | 24 / 9.3 |

Qwen3.5-9B (auto / gpu): 2323 / 43.8 and 2319 / 43.8 vs llama-bench 2486 / 43.8 and 2462 / 43.8. These
match run 11, so reading plain answers with the parser did not change speed.

## The answers that failed in run 11

| Model | Check | Run 11 | Run 12 |
|---|---|---|---|
| gpt-oss-20b | thinking | `<\|start\|>assistant<\|channel\|>final<\|message\|>No.` | `No.` (reasoning kept apart) |
| gpt-oss-20b | answer after the tool result | its reasoning ("We have a conversation...") | "Temperature: 18 °C, Sky: cloudy" |
| Phi-4-mini | answer after the tool result | "I will need to call the tool again" | "18°C with cloudy skies" |

The check "answer after the tool result" now also fails on control tokens and on reasoning text in the
answer.

## Compare row for gpt-oss-20b (Cluaiz only)

Cold request 69.6 s, prompt 2651 t/s, generation 81.1 t/s, GPU memory 12.4 GB; replies are plain text. Ollama
(78.5 t/s) and LM Studio (62.2 t/s) are the run 11 rows on the same file. A Cluaiz request now generated
157-227 tokens with `max_tokens` 64: gpt-oss reasons even with thinking off, and its reasoning is not counted
in `max_tokens`, as for every model.
