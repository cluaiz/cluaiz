# Run 39: does a small model call the tool, or make up the answer? (2026-11-21)

Windows 11 (build 26300), RTX 3050 4 GB, Engine 0.1.0 debug build of `main` at `2ae462e3`, isolated
`CLUAIZ_HOME` on port 8011, `POST /v1/chat/completions` with 5 tools in every request, temperature 0.
Script: `probe.py`; output: `results.txt`; every reply: `<model>.jsonl`.

40 prompts: 24 need one obvious tool, 8 need a tool but say it indirectly ("Do I need an umbrella in London
today?"), 8 need no tool. A call counts only when the tool name and the expected arguments are right.

| Model | Correct | Obvious (24) | Indirect (8) | No tool needed (8) | Median per request |
|---|---:|---:|---:|---:|---:|
| Qwen3 0.6B Q4_K_M, thinking off | 27 / 40 | 18 | 1 | 8 | 2.04 s |
| Qwen3 0.6B Q4_K_M, thinking on | 34 / 40 | 20 | 7 | 7 | 3.56 s |

With thinking off, 8 of the 32 prompts that needed a tool got a plain-text answer instead, mostly invented:
"Is it raining in Mumbai?" got "It is currently raining in Mumbai."; "How hot is it in Cairo, in celsius?"
got "It is currently 35°C in Cairo, in Celsius."; "My flight costs 300 EUR; how much is that in USD?" got
"approximately 420 USD". Wrong arguments were mostly names instead of codes (`"british pounds"` for `GBP`)
or extra text (`"24h 12:30"`).

## Not proven

- Only Qwen3 0.6B; Qwen3 1.7B, Qwen3 4B and Llama 3.2 3B were downloaded but not run.
- One run per prompt at temperature 0; other engines were not run on the same prompts.
