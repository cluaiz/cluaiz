# Run 83: does the order of the image tool hint matter?

Date: 2026-11-27. Machine: cluaiz-l4b. Models: Qwen3.5-0.8B `q08` and a 4B `q4` (see `raw/*-pull.txt`).
Question: "what colour is this image" with a red image. Three prompt orders: H1 text hint then image,
H2 no hint, H3 image then hint. 1 probe + 5 runs each, order shuffled.

## Result: 30/30 for both models

| Model | H1 | H2 | H3 |
|---|---|---|---|
| q08 | 5/5 | 5/5 | 5/5 |
| q4 | 5/5 | 5/5 | 5/5 |

All answers "red", no tool calls. Prompt tokens 75 with hint, 37 without. Time 0.2-0.3 s after the first
request (first 2.3 s / 3.2 s, load). Reading: hint order makes no difference; the hint costs 38 tokens.
Raw: `raw/q08.txt`, `raw/q4.txt`, `raw/*.jsonl`. Rerun: `run.sh`.
