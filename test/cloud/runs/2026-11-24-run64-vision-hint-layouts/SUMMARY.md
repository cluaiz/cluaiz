# Run 64: image + tool hint layouts on the L4 (run 50 follow-up), measurement only

Date: 2026-11-24 (10:20 to 10:35 UTC). Run 50 (`../2026-11-23-run50-vision-vram/SUMMARY.md`, Windows laptop) found on a
GPU projector with the Engine's tool hint: text part before the image 0 / 9 answered red, without the hint 3 / 3,
image first with the hint 6 / 6. This run repeats that with 2 models x 3 layouts x 5 runs, plus a tool check.
No Engine code was changed.

Files: `vision_hint.py` (layouts + tool check), `tool_control.py` (tool question with and without the image),
`probe64.sh` (pre-check of the hint), `summarize_run64.py` (tables), `raw/` (per model: `.jsonl` every request with its
full reply (image replaced by its length), `.txt` one line per request, `_r<N>.log` server log per round with `-vv`;
`*.tools.*` and `*_tools_r<N>.log` from `tool_control.py`; `install_*.txt`, `precheck_*`, `run64.out`, `tables.md`).
User name replaced by `<user>`.

## Setup

| Item | Value |
|---|---|
| Machine | GCP `g2-standard-8` (<vps>): NVIDIA L4 23 GB (nothing else on the GPU; `vram_hog.py` stopped after run 63), Debian 13, kernel 6.12.117 |
| Binary | the run 63 release build: Engine `9dfd4f46`, fork `96b0cfb2d` (contains run 50's newline fix `e936c574` and the projector VRAM fix) |
| Home / port | isolated `CLUAIZ_HOME=~/ch-run64` (new), port 8011, `CLUAIZ_HOST=127.0.0.1`, auth settings at their defaults |
| Tools in the home | `plugin install math --yes` and `mcp install filesystem --yes` (run 49's two, from the registry). Without them a fresh home gave no tool hint (pre-check, first request); with them `context_report.tools.hint_tokens` = 37 |
| Models | `qwen3.5-0.8b-gguf-Q4_K_M` (`unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M`, run 50's model, mmproj F16 195 MiB) and `qwen3.5-4b-gguf-Q4_K_M` (`unsloth/Qwen3.5-4B-GGUF:Q4_K_M`, the example in `cluaiz pull --help`, mmproj F16 641 MiB) |
| Projector | GPU in every round (log: `Projector loaded placement=Gpu`, 196 MiB / 642 MiB taken) |
| Image / question | red 448x448 PNG, "What colour is this image? One word." (run 50's), temperature 0, `think_mode: off`, `max_tokens` 64 |
| Rounds | 5 per model; each round a fresh `cluaiz -vv serve`, the 3 layouts in rotated order, then the tool check. Round 1 also sent each layout once with `return_prompt: true` (not counted) |

Layouts (the client's message; the Engine puts its hint, when it has one, in front as its own part):

| Layout | Message parts | Extra |
|---|---|---|
| L1 | text, image | - (today's order, with the hint) |
| L2 | text, image | `tool_choice: "none"` (no hint) |
| L3 | image, text | - (with the hint) |

## Results: red answers

| Model | L1 text first, hint | L2 text first, no hint | L3 image first, hint |
|---|---|---|---|
| Qwen3.5 0.8B | 5 / 5 ("The image is red.") | 5 / 5 ("The image is red.") | 5 / 5 ("red") |
| Qwen3.5 4B | 5 / 5 ("red") | 5 / 5 ("red") | 5 / 5 ("red") |

Prompt size per layout (same for both models): `usage.prompt_tokens` 228 / 161 / 227, llama.cpp `timings.prompt_n`
410 / 343 / 409 (L1 and L3 carry about 65 more: `context_report` gives hint_tokens 37 and system_tools_tokens 28).

Run 50's failure (0 / 9 with the text first and the hint) did **not** appear here: 10 / 10 across the two models.
Differences from run 50 that this run cannot separate: Linux and the L4 against Windows and a 4 GB RTX 3050, a release
build of `9dfd4f46` against a debug build of `e936c574`, the default 1 GiB projector estimate with plenty of VRAM
against a lowered estimate with a thin margin, and the hint's text (run 50 kept it in `raw/engine_tools_hint.txt`;
here only its length, 37 tokens, is known, because `return_prompt` returned no rendered prompt in this reply).

## Results: tool check (layout 3 order) and its control

Question: "What is the weather in Paris right now? Use the get_weather tool.", with a client function
`get_weather(city)` in `tools`. Pass = the reply has a `get_weather` tool call.

| Model | Image first + tools (in `vision_hint.py`) | Image first + tools (`tool_control.py`) | Text only + tools (`tool_control.py`) |
|---|---|---|---|
| Qwen3.5 0.8B | 0 / 5 (empty content, `reasoning_content` "get_weather") | 0 / 5 (same) | **5 / 5** `get_weather({"city":"Paris"})` |
| Qwen3.5 4B | 0 / 5 ("I don't have access to a get_weather tool in this project. I can only use math and filesystem tools.") | 0 / 5 (same) | **5 / 5** `get_weather({"city":"Paris"})` |

`timings.prompt_n` (tokens llama.cpp processed): **933** for the text-only request, **415** for the image request
(about 233 text tokens plus the image). `context_report.tools.template_tools_tokens` was 403 in both. So on the image
requests the function schemas (the client's `get_weather` and the Engine's own functions) did not reach the prompt,
while the hint and the system tool text did. The 4B's answer names the tools listed in the hint text, not
`get_weather`. Where the schemas are dropped was not traced: `engine/llama/src/native/media.rs:480`
(`MediaInput::prepare`) passes `tools: request.tools` on, so the cut is elsewhere in the media path.

Side observation: `usage.prompt_tokens` of the 0.8B text-only request was 218, 233 or 933 for the same 933-token
prompt (233 when it ran after an image request on the same server), so `usage.prompt_tokens` is not reliable here;
`timings.prompt_n` is.

## What this does not prove

- That the hint never hurts: run 50's 0 / 9 is not explained, only not reproduced on this machine and build.
- Other vision models (non-Qwen projectors), the CPU projector, Windows, macOS, a 4 GB GPU, other images or questions.
- The cause of the missing tool schemas on image requests; only the effect (prompt_n 415 against 933, 0 / 20 tool calls
  with the image against 10 / 10 without).
- The Engine's own tools (math plugin, filesystem MCP) on image requests: only the client function was checked.

## Pending

| Item | Why |
|---|---|
| Tool schemas are not in the prompt of image requests (0 / 20 tool calls with an image, 10 / 10 without) | an image message cannot call any function today; owner decision before code |
| Run 50's text-first failure | not reproduced (10 / 10 here); repeating on the run 50 laptop with this build would tell whether the machine, the build or the hint text caused it |
