# Run 77: tool count and small models (partial, 2026-11-25)

This is an incomplete test record. Seven of the ten requested models have complete local raw records. SmolLM3 was running on the VM when remote access was lost; its completion and scores are **?**. Qwen3.5 0.8B and 2B were downloaded, but the first runner mistook their main GGUF plus vision projector for two main files and stopped before scoring. The file selector is corrected in `run_matrix.py`; those two models still need to run. Do not quote this run as a complete model-size threshold.

## Machine, build and method

- Machine: GCP VM `cluaiz-32gb`, Linux, 31 GiB RAM, NVIDIA L4 with 23,034 MiB VRAM. Only this VM ran inference. Before the scored matrix, free RAM was 32,459,182,080 bytes and free VRAM 22,615 MiB. End resources are **?** because VM access was interrupted. Disk free after the build was 8.0 GB and fell to 3.7 GB with two Qwen3 4B model copies; models were pulled and removed sequentially from isolated run homes.
- Source: current local Engine commit `f8d6b4b2073c77cae4a3007b9c9eb8bbefaa2e1f` archived without private fixtures; llama.cpp submodule `96b0cfb2d8866e1b94e482d6d6c9c4eace1747f8`. Built on the VM with `cargo build -p cmd`, dev profile, 28m 59s. Binary SHA-256: `7f5d690845d7e6775169729fc7f166ee27d71ddb4035afdf8ecc952c3e07f76a`. No Engine source was edited. The VM's older checkout was left alone.
- Server: port 8013, isolated `CLUAIZ_HOME` under `~/run77/homeA` for A/C and `~/run77/home` for B. No provider keys. The owner's server and default home were not used. The math plugin and filesystem MCP were installed only in B's home. B's pilot `context_report.plan.project.read_item` was `true`; the installed tools are offered contextually, not as a fixed number on every prompt.
- A: five Run58 client tools only via `/api/chat`. B: the same five client tools plus Engine context tools via `/v1/chat/completions`. C: the five plus 15 realistic client schemas, 20 total, via `/api/chat`. The two routes share the Engine chat pipeline but differ in prompt policy; therefore A/B differences cannot be attributed only to the count of schemas. The dummy tools were offered as client schemas and were not executed.
- `run58_probe.py` is the unchanged Run58 probe. `matrix_probe.py` extracts its exact 40 prompts (24 obvious, 8 indirect, 8 no-tool), five tool schemas, `same()` and `grade()` without running its CLI loop. Temperature 0, 256 max output tokens when thinking off and 1536 when on. A required call is correct only with the right first tool and expected arguments. No-tool is correct only if neither a client nor an Engine tool call occurred. Each model/setup/mode is one 40-request pass, no repeated seeds. A client call is returned to the client, so it normally has zero Engine continuation steps.
- Each raw row holds the prompt, expected call, complete API reply, grade, duration and Engine log lines. `analyze.py` recomputes all counts. A loop prompt means an identical Engine tool name **and payload** appears at least twice in one request. Steps count the Engine's `generating again with the tool result` log lines. `math.json` records the separate plain `6*7` B request. All copied raw files are under 1 MB each. VM account paths and IPs were redacted before transfer. The initial two Qwen 0.6B B files were damaged by the first redactor; they were deleted and repeated as `B2`. Only `B2` is scored.

## Exact model files measured

All files below were registered through `cluaiz pull` in each isolated home, and A/B hashes matched. Q4_K_M is a compressed GGUF format; it is the file format/precision choice, not model size. Thinking was run on Qwen3 models; LFM3, Llama and Gemma were run off only. SmolLM3's on/off data is not yet local.

| Model | Hugging Face repository | Main GGUF file | Bytes | SHA-256 |
|---|---|---|---:|---|
| Qwen3 0.6B | `unsloth/Qwen3-0.6B-GGUF` | `Qwen3-0.6B-Q4_K_M.gguf` | 396705472 | `ac2d97712095a558e31573f62f466a3f9d93990898b0ec79d7c974c1780d524a` |
| Qwen3 1.7B | `unsloth/Qwen3-1.7B-GGUF` | `Qwen3-1.7B-Q4_K_M.gguf` | 1107409472 | `b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897` |
| Qwen3 4B | `unsloth/Qwen3-4B-GGUF` | `Qwen3-4B-Q4_K_M.gguf` | 2497281312 | `f6f851777709861056efcdad3af01da38b31223a3ba26e61a4f8bf3a2195813a` |
| LFM3 1.7B | `LiquidAI/LFM3-1.7B-GGUF` | `LFM3-1.7B-Q4_K_M.gguf` | 1099627808 | `3793e1e6e2266919d6e015287221b41100175a98eddaba92e111aed3b05c93c4` |
| Llama 3.2 1B | `bartowski/Llama-3.2-1B-Instruct-GGUF` | `Llama-3.2-1B-Instruct-Q4_K_M.gguf` | 807694464 | `6f85a640a97cf2bf5b8e764087b1e83da0fdb51d7c9fab7d0fece9385611df83` |
| Llama 3.2 3B | `bartowski/Llama-3.2-3B-Instruct-GGUF` | `Llama-3.2-3B-Instruct-Q4_K_M.gguf` | 2019377696 | `6c1a2b41161032677be168d354123594c0e6e67d2b9227c84f296ad037c728ff` |
| Gemma 3 1B | `ggml-org/gemma-3-1b-it-GGUF` | `gemma-3-1b-it-Q4_K_M.gguf` | 806058240 | `8ccc5cd1f1b3602548715ae25a66ed73fd5dc68a210412eea643eb20eb75a135` |

## Strict scores from 30 complete passes

The model names abbreviate Qwen3 as `qwen`, LFM3 as `lfm`, Llama 3.2 as `llama`, and Gemma 3 as `gemma`. `B2` is the valid Qwen 0.6B B rerun. Wrong no-tool counts any client or Engine call on the eight prompts needing no tool. Loop prompts count identical internal calls repeated within a request. Median is observed seconds per request, including warm/cold effects; it is not a speed comparison against another runtime.

| Model/mode/setup | Correct /40 | Obvious /24 | Indirect /8 | No-tool /8 | Wrong no-tool | Loop prompts | Steps total | Median s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| gemma1-off-A | 7 | 0 | 0 | 7 | 1 | 0 | 0 | 0.22 |
| gemma1-off-B | 8 | 0 | 0 | 8 | 0 | 0 | 0 | 0.21 |
| gemma1-off-C | 8 | 0 | 0 | 8 | 0 | 0 | 0 | 0.18 |
| lfm17-off-A | 34 | 23 | 8 | 3 | 5 | 0 | 0 | 0.72 |
| lfm17-off-B | 30 | 22 | 8 | 0 | 8 | 0 | 17 | 0.90 |
| lfm17-off-C | 33 | 24 | 8 | 1 | 7 | 0 | 0 | 0.81 |
| llama1-off-A | 32 | 21 | 6 | 5 | 3 | 0 | 0 | 0.17 |
| llama1-off-B | 6 | 4 | 1 | 1 | 7 | 29 | 414 | 0.60 |
| llama1-off-C | 19 | 12 | 3 | 4 | 4 | 0 | 0 | 0.20 |
| llama3-off-A | 34 | 24 | 8 | 2 | 6 | 0 | 0 | 0.30 |
| llama3-off-B | 23 | 20 | 2 | 1 | 7 | 11 | 248 | 0.40 |
| llama3-off-C | 34 | 24 | 8 | 2 | 6 | 0 | 0 | 0.32 |
| qwen06-off-A | 27 | 17 | 2 | 8 | 0 | 0 | 0 | 0.66 |
| qwen06-off-B2 | 21 | 13 | 1 | 7 | 1 | 0 | 1 | 0.21 |
| qwen06-off-C | 25 | 17 | 1 | 7 | 1 | 0 | 0 | 0.79 |
| qwen06-on-A | 32 | 19 | 6 | 7 | 1 | 0 | 0 | 1.43 |
| qwen06-on-B2 | 34 | 22 | 5 | 7 | 1 | 0 | 2 | 1.54 |
| qwen06-on-C | 31 | 19 | 6 | 6 | 2 | 0 | 0 | 1.65 |
| qwen17-off-A | 32 | 20 | 4 | 8 | 0 | 0 | 0 | 0.93 |
| qwen17-off-B | 31 | 20 | 5 | 6 | 2 | 1 | 18 | 1.18 |
| qwen17-off-C | 32 | 20 | 5 | 7 | 1 | 0 | 0 | 1.09 |
| qwen17-on-A | 38 | 23 | 8 | 7 | 1 | 0 | 0 | 1.81 |
| qwen17-on-B | 37 | 24 | 8 | 5 | 3 | 2 | 10 | 2.40 |
| qwen17-on-C | 35 | 23 | 7 | 5 | 3 | 0 | 0 | 2.04 |
| qwen4-off-A | 37 | 22 | 7 | 8 | 0 | 0 | 0 | 1.31 |
| qwen4-off-B | 38 | 23 | 8 | 7 | 1 | 1 | 4 | 1.38 |
| qwen4-off-C | 35 | 22 | 6 | 7 | 1 | 0 | 0 | 1.23 |
| qwen4-on-A | 38 | 23 | 7 | 8 | 0 | 0 | 0 | 2.83 |
| qwen4-on-B | 39 | 24 | 8 | 7 | 1 | 0 | 0 | 3.04 |
| qwen4-on-C | 36 | 23 | 6 | 7 | 1 | 0 | 0 | 3.12 |

## What the raw replies show

- Llama 3.2 1B B called `read_item` repeatedly on 29/40 prompts, with 414 total continuation steps. Its Paris-weather request repeated `read_item` with weather-shaped memory arguments, hit the step limit after 99 steps, and took 26.04 s. Llama 3.2 3B B had 11 loop prompts and 248 steps. These are observed loops, not proof that adding any Engine tool always causes them.
- Gemma 3 1B returned tool-shaped JSON in ordinary `content` for the 32 required-call prompts in A, instead of parsed `tool_calls`. For example its Paris reply began a JSON block with `"name": "get_weather"`. The Run58 grading correctly counts those as no call.
- Qwen3 0.6B off A answered `It is currently raining in Mumbai.` and `It is currently 35°C in Cairo, Cairo.` without calling `get_weather`. These are model assertions, not verified weather facts.
- For B's plain `6*7`, Qwen3 0.6B off returned `6*7` with 0 steps in 0.06 s; on returned `6*7 = 42` with 0 steps in 0.71 s. LFM3 reached the math plugin after four Engine steps and returned 42 in 6.25 s. The per-mode `math.json` files hold every reply, step count and duration. Other measured modes are in `metrics.json`; the answer is not assumed correct merely because a plugin was installed.

## Interpretation and limits

There is no universal parameter-size boundary in these seven models. On this build and prompt set, Qwen3 4B with thinking on scored 38/40 in A, 39/40 in B, 36/40 in C; Qwen3 1.7B with thinking on scored 38/40, 37/40, 35/40. Llama 1B fell from 32/40 in A to 6/40 in B, while its C score was 19/40. LFM3 off handled all eight indirect prompts in every setup but called tools unnecessarily on many no-tool prompts. The Engine can test a model-specific tool budget; these one-pass measurements do not yet justify a global cutoff. A/C versus B also changes endpoint prompt policy and contextual Engine tool selection, so it does not isolate count alone. No external weather, exchange, email or filesystem action was executed; only tool choice and arguments were scored.

## Replay, remaining work and access interruption

From a VM test directory with the same source build and registered GGUF, use `python3 matrix_probe.py --exe <binary> --home <isolated-home> --out <raw-dir> --model <id> --setup A|B|C --think on|off`. The script starts its own isolated server on port 8013. `run_matrix.py` pulls, runs and removes models sequentially from run77 homes. `python3 analyze.py` recomputes `metrics.json` and the table. These scripts are test-only; they do not modify Engine source.

Pending: collect and validate SmolLM3 off/on output; rerun Qwen3.5 0.8B and 2B with the corrected main-GGUF selector; record end RAM/VRAM; rerun `analyze.py`; decide whether this one-pass evidence merits an Engine tool-budget change in a separate code task. The last VM command was not executed because automatic approval review reported a usage limit. The existing SSH session then became unavailable. This is an access limit, not a measured model failure. No final model-size recommendation is claimed yet.
