# Run 58: small-model tool calls, model and system-hint comparison (2026-11-24)

## Setup and limits

Windows 11 build 26300, AMD Ryzen 7 7435HS, 23.69 GiB RAM, NVIDIA GeForce RTX 3050 A Laptop GPU with 4094 MiB VRAM. The scored requests used the existing `target/debug/cluaiz.exe` on `127.0.0.1:8014` with `CLUAIZ_HOME=C:\Users\Aryan\AppData\Local\cluaiz-test\run58-small-tools`. The binary SHA-256 was `53CE960CBB8D6B31A792D709ED1621E728E6C10F43E53D78C15C1DDFAAA24065`; its source commit is **?**. The executable's last-write time was 2026-11-24 10:30:28 local, before the first scored server start at 13:08 local. The checkout changed during the study, so the checkout commit is not a valid substitute for the binary's source commit. No Engine source or build was changed for this run. No provider keys were used. The owner's port 8000, port 8011 and `~/.cluaiz` were not used.

At the first scored start, free RAM/VRAM were 5.15 GiB / 2624 MiB; after the final hint run they were 7.73 GiB / 2862 MiB. Across individual run starts, free RAM ranged from 3.99 to 7.69 GiB and free VRAM from 2233 to 2829 MiB. Other work on this machine changed resource availability. The per-request medians below are observed timings, not clean relative-speed measurements. `probe.py` reports the upper middle of 40 sorted request durations, following Run 39.

The source is Run 39's `probe.py`: the same 40 user prompts (24 obvious tool, 8 indirect tool, 8 no tool), the same five tool schemas, `temperature: 0`, `temporary_chat: strict`, 256 output tokens with thinking off and 1536 with thinking on, and the same `grade()` and `same()` functions. A required call is correct only when the first call has the expected tool name and every expected argument value. A no-tool prompt is correct when no call is made. This copy adds full raw-reply capture and an optional single system message; it does not alter tasks or grading. One pass per prompt and mode was run, with no repetitions or competitor-engine comparison.

## Exact GGUF files

All six files were downloaded and registered by `cluaiz pull` into the isolated home. The last two sources were checked on Hugging Face before downloading. MiniCPM5's official model card documents think and no-think modes; no LFM3 thinking mode was established, so LFM3 was run off only. Llama 3.2 3B was run off only.

| Model ID | Hugging Face repository | Exact file | Quant | Bytes |
|---|---|---|---|---:|
| `qwen3-0.6b-gguf-Q4_K_M` | `unsloth/Qwen3-0.6B-GGUF` | `Qwen3-0.6B-Q4_K_M.gguf` | Q4_K_M | 396,705,472 |
| `qwen3-1.7b-gguf-Q4_K_M` | `unsloth/Qwen3-1.7B-GGUF` | `Qwen3-1.7B-Q4_K_M.gguf` | Q4_K_M | 1,107,409,472 |
| `qwen3-4b-gguf-Q4_K_M` | `unsloth/Qwen3-4B-GGUF` | `Qwen3-4B-Q4_K_M.gguf` | Q4_K_M | 2,497,281,312 |
| `llama-3.2-3b-instruct-gguf-Q4_K_M` | `bartowski/Llama-3.2-3B-Instruct-GGUF` | `Llama-3.2-3B-Instruct-Q4_K_M.gguf` | Q4_K_M | 2,019,377,696 |
| `lfm3-1.7b-gguf-Q8_0` | `LiquidAI/LFM3-1.7B-GGUF` | `LFM3-1.7B-Q8_0.gguf` | Q8_0 | 1,846,238,496 |
| `minicpm5-1b-gguf-Q8_0` | `openbmb/MiniCPM5-1B-GGUF` | `MiniCPM5-1B-Q8_0.gguf` | Q8_0 | 1,153,529,216 |

Official file pages: [LiquidAI LFM3 Q8](https://huggingface.co/LiquidAI/LFM3-1.7B-GGUF/blob/main/LFM3-1.7B-Q8_0.gguf), [openbmb MiniCPM5 Q8](https://huggingface.co/openbmb/MiniCPM5-1B-GGUF/blob/main/MiniCPM5-1B-Q8_0.gguf), and [MiniCPM5 model card](https://huggingface.co/openbmb/MiniCPM5-1B-GGUF). Accessed 2026-11-24. The repository and exact file names above also appeared in the CLI pull output and in the isolated vault.

## Baseline: no extra system line

All rows share the setup above. Scores are correct out of 40; obvious is out of 24, indirect out of 8, and no-tool out of 8. Each row is one 40-request run, with full raw API replies in `baseline/raw/` and graded replies in `baseline/`.

| Model and thinking | Correct /40 | Obvious /24 | Indirect /8 | No-tool /8 | Median s/request |
|---|---:|---:|---:|---:|---:|
| Qwen3 0.6B Q4_K_M, off | 27 | 18 | 1 | 8 | 3.12 |
| Qwen3 0.6B Q4_K_M, on | 34 | 20 | 7 | 7 | 4.04 |
| Qwen3 1.7B Q4_K_M, off | 31 | 19 | 4 | 8 | 2.95 |
| Qwen3 1.7B Q4_K_M, on | **40** | 24 | 8 | 8 | 4.66 |
| Qwen3 4B Q4_K_M, off | 37 | 23 | 6 | 8 | 4.84 |
| Qwen3 4B Q4_K_M, on | 39 | 23 | 8 | 8 | 12.37 |
| Llama 3.2 3B Instruct Q4_K_M, off | 33 | 24 | 8 | 1 | 0.77 |
| LFM3 1.7B Q8_0, off | 38 | 23 | 8 | 7 | 2.12 |
| MiniCPM5 1B Q8_0, off | 28 | 22 | 3 | 3 | 2.76 |
| MiniCPM5 1B Q8_0, on | 35 | 22 | 6 | 7 | 5.22 |

Qwen3 1.7B with thinking on had the highest strict score in this one-pass set. Llama made the required calls correctly on all 32 tool-needed prompts, but called a tool on seven of eight no-tool prompts. LFM3 loaded and returned parsed tool calls on this binary; it was not skipped.

## The one-line system hint

The only request change was this one system message before the unchanged user message: `For weather, rain, temperature or umbrella questions use get_weather; for money conversion use convert_currency; for reminders use create_reminder; for finding documents use search_files; for sending mail use send_email. For questions needing none of these actions, do not call a tool.` The same 40 prompts, five tools, scoring, model files and binary were used. Full and graded replies are in `hint/raw/` and `hint/`.

| Model and thinking | Baseline correct /40 | Hint correct /40 | Hint obvious /24 | Hint indirect /8 | Hint no-tool /8 | Hint median s/request |
|---|---:|---:|---:|---:|---:|---:|
| Qwen3 0.6B, off | 27 | 28 | 17 | 4 | 7 | 2.31 |
| Qwen3 0.6B, on | 34 | 34 | 21 | 6 | 7 | 3.04 |
| Qwen3 1.7B, off | 31 | 33 | 21 | 4 | 8 | 2.46 |
| Qwen3 1.7B, on | 40 | 38 | 23 | 8 | 7 | 3.86 |

The hint helped both off modes by one and two correct responses, left 0.6B on unchanged, and reduced 1.7B on by two. It did not consistently improve tool reliability. A one-pass difference of one or two prompts is not a general model-quality claim.

## Answers asserted without the needed tool

These are observed response excerpts, not live weather or exchange-rate facts. Baseline Qwen3 0.6B off said `It is currently raining in Mumbai.`, `It is currently 35°C in Cairo, in Celsius.`, and `The cost of your flight is 300 EUR, which is approximately 420 USD.` without calling the offered tool. Baseline Qwen3 1.7B off asserted `The current weather in Cairo is 32 degrees Celsius.` without a tool call. With the hint, Qwen3 1.7B off said `The weather in Mumbai is currently clear. There is no rain.` and `The weather in London today is likely to be rainy. I recommend carrying an umbrella.` without calling `get_weather`. The full raw replies show each prompt and response.

## Evidence and replay

- `baseline/results.txt` and `hint/results.txt` are the unchanged probe's score output. There are ten baseline and four hint graded JSONL files, each with 40 rows, and matching full raw-reply files, each with 40 rows. There were no `http_error` verdicts. Each raw file and each Engine log is under 1 MB, so all are stored in this run folder. No raw file needed to stay outside git.
- `runner.py` starts and stops the isolated server and records free RAM/VRAM. `baseline/setup.json` is the final baseline invocation's resource snapshot; `hint/setup.json` is the hint invocation's snapshot. Earlier individual start/end values were captured in the execution session; see `resource-snapshots.md`.
- Replay after the six models are registered in an isolated home: `python runner.py --exe <absolute-cluaiz-exe> --home <isolated-home> --port 8014 --variant baseline <model-id>[:think]`; then repeat with `--variant hint` for the two Qwen models. The CLI pull commands used `<repo>:<quant>` as listed above. The script refuses ports other than 8013/8014 and the default home.
- The first unscored server attempt was sandboxed and could not access localhost or create a session token in the isolated home. It produced no prompt scores. The scored rerun used a permitted execution context. The Llama and MiniCPM downloads experienced connection drops and resumed through the CLI. All six final GGUF files are complete.

## Not proven

These counts are one pass per prompt at temperature zero; other seeds, prompts, tools, machines and runtimes were not tested. The build source commit is unknown. A free-RAM/VRAM swing and concurrent work prevent a clean speed ranking from these medians. The test did not call the external tools to fetch real weather, rates, mail or files; it scored the model's decision and arguments only.
