# Run 50: image requests on a 4 GB GPU, before and after the projector VRAM fix (2026-11-23)

Windows 11 (build 26300), Ryzen 7 7435HS, 23.7 GB RAM, RTX 3050 4 GB (the run 49 laptop). Model
`unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M` with `mmproj-F16.gguf` (195 MiB), a red 448x448 PNG, temperature 0,
`think_mode: off`, question "What colour is this image? One word." Isolated home of run 49
(`%LOCALAPPDATA%\cluaiz-test\run49-e2e`, with the `math` plugin and `filesystem` MCP of run 49 installed),
port 8014, auth settings at their defaults. Each scenario starts its own server, so "fresh server" is real.

Binaries: `target\debug\deps\cluaiz.exe` (`target\debug\cluaiz.exe` is locked by another server).
- **before**: the run 49 build (`f38411a8` code), built 17:06.
- **after**: debug `cargo build -p cmd` of `e936c574` (the fix). The working tree also had another agent's
  uncommitted `engine/llama/build.rs` (CPU backends built with clang) and llama.cpp pointer `8676a916e`, so the
  after build loads clang-built CPU modules; the GPU path and the changed Rust code do not depend on them. The
  first build attempt failed copying `ggml-cpu-haswell.dll`, held by PID 44004; it was renamed to
  `ggml-cpu-haswell.dll.old-pid44004` like run 49's files (`raw/build_attempt1_locked_dll.txt`, `raw/build.txt`).

Scripts: `vision_vram.py` (scenarios, GPU memory from `nvidia-smi` after each request), `sweep_workspace.sh`
(sets `media.projector_workspace_bytes` in the test home, runs a scenario, restores the settings),
`official_server.py` (official llama-server b12428 with the same prompts).

## Bug 1: image requests after the first get HTTP 500 on a GPU projector

**Cause** (`engine/llama/src/native/session.rs`, before `e936c574`, lines 427-435): each request on a GPU
projector needed `workspace` (1 GiB default) + the safety buffer (512 MiB here) free right now. The load plan
(`engine/plan/src/media_budget.rs`, `MediaBudget::plan`) asked for weights + workspace + buffer free before the
load. After the load the projector holds its weights, and after its first encode its compute buffers (warmup is
off, `media.rs` `params.warmup = false`), so the same estimate was charged twice. When the load passed with a thin
margin, the first request passed and every later one was refused; the reuse path (`ensure_projector`, RAM check
only) never planned the projector again.

Measured with the new log lines (after build, `raw/sweep/after_ws{560,600,800}_five_images.log`): the load took
194-208 MiB, the first encode 41-45 MiB more, the second 4-7 MiB, later ones 0 (`held_by_projector` 237-257 MiB).

**Repro on the before binary.** With the default 1 GiB estimate the projector went to the CPU on this day (the
load needs 195 + 1024 + 512 = 1731 MiB free; 1261-1371 MiB were free with the model loaded), so the 500 cannot
appear. `sweep_workspace.sh` lowered the estimate to put the projector on the GPU with a thin margin, as run 49
had by chance:

| Estimate | Before binary (image-first, text-first, text-first) | Raw |
|---|---|---|
| 528, 560, 592, 624 MiB | 200, 200, 200 | `raw/sweep/before_ws5*.txt`, `before_ws6*.txt` |
| 704 MiB | 200, 200, **500 "Insufficient live VRAM for media request workspace"**; free 1213 MiB < 704 + 512 | `raw/sweep/before_ws704_image_first.txt` |
| 736, 768, 800 MiB | 200 x 3 (projector on the CPU, judged from the GPU memory used: 1.24-1.32 GB for model and projector, as much as the model alone) | `raw/sweep/before_ws7*.txt`, `before_ws800*.txt` |

**Fix** (`e936c574`): the session keeps `projector_vram_held` (the free-VRAM drop across the load plus the drop
across each request, capped at weights + workspace); the request check asks for weights + workspace - held +
buffer. That is the load rule checked again with the projector's own memory added back. If it still fails
(another model or program took VRAM), `ensure_projector` unloads the idle projector and plans it again, as it did
for RAM.

**After** (5 image requests in a row on a fresh server, text-first and image-first alternating):

| Estimate | Placement (log) | HTTP | Old rule before request 2 | New rule before request 2 | Raw |
|---|---|---|---|---|---|
| 560 MiB | GPU, 1467 MiB free at load | 5 / 5 200 | 1072 asked, 1216 free | 1017 asked | `raw/sweep/after_ws560_five_images.*` |
| 600 MiB | GPU, 1401 MiB free at load | 5 / 5 200 | 1112 asked, 1163 free | 1070 asked | `raw/sweep/after_ws600_five_images.*` |
| 800 MiB | GPU, 1536 MiB free at load | 5 / 5 200 | **1312 asked, 1295 free: the old binary would refuse requests 2-5** | 1266 asked | `raw/sweep/after_ws800_five_images.*` |
| 1 GiB (default), 3 scenarios | CPU (1339-1428 MiB free at load) | 12 / 12 200 (11 image requests) | - | - | `raw/after_*.txt`, `raw/after_*.log` |

"Old rule" is computed from the logged free VRAM with the before formula (estimate + 512 MiB); the before binary
returned the 500 at the same formula in the 704 MiB row above.

Other after sweeps were spoiled by the machine, not by the Engine: 640-708 MiB put the projector on the CPU;
708, 576, 816 and 832 MiB failed with "Media needs ... host bytes; live RAM budget is ..." while another agent's
release build and `cluaiz bench` took the RAM (0.9-1.9 GB free); 592 and 784 MiB could not load the chat model
(0.01-0.08 GB short of VRAM). All kept in `raw/sweep/`.

## Bug 2: with the text part before the image, the model says it cannot see images

**Cause** (`engine/llama/src/native/media.rs`, `MediaInput::prepare`): text parts of one message were glued with
nothing between them. The Engine puts its tool hint in front of the user message as its own part
(`api/src/handlers/chat/context_plan.rs`, `prepend_text`), so text-first became
`...</tools_hint>What colour is this image? One word.[image]`. The marker was not dropped or misordered:
`prompt_tokens` was the same in both orders (239) and the log shows the image tokens processed. llama-server joins
text parts with a newline and none next to a media marker (`concat_content_parts`, pinned `common/chat.cpp`);
the fix does the same (`prompt_tokens` 240 for text-first after it).

| Fresh servers, default settings (CPU projector) | text-first answered red | image-first answered red |
|---|---|---|
| before (`raw/before_*.txt`) | 0 / 7 ("I cannot identify the color of an image. I am an AI assistant ...") | 4 / 4 |
| after (`raw/after_*.txt`) | **7 / 7** ("The image is red.") | 4 / 4 |

That includes "text chat first, then 3 images" (`raw/*_text_then.txt`): all 200 before and after.

**Still open:** on a GPU projector with the tool hint, text-first still answered "I cannot identify ..." (9 / 9 in
the 560, 600 and 800 MiB rows), image-first answered red (6 / 6). Without the hint (`tool_choice: "none"`) on the
GPU projector, text-first answered red 3 / 3 (`raw/sweep/after_nohint_ws560_five_images.*`). Official llama-server
b12428 (`raw/official_server_gpu.txt`) answered red in both orders for the plain question (4 / 4), and made tool
calls in both orders (4 / 4) when given the Engine's own system prompt and hint (`raw/engine_system_prompt.txt`,
`raw/engine_tools_hint.txt`, taken from a `return_prompt` reply, `raw/prompt_probe_text_only.json`). So the hint
steers this 0.8B model; it is not a marker problem. Whether image requests should get the tool hint is the
owner's call.

## What this run does not prove

- The reclaim path (a GPU projector moved to the CPU after a failed VRAM check): not hit.
- A large image after small ones on a GPU projector (growth is counted after a request ran).
- The 1 GiB default estimate on the GPU: the projector went to the CPU on this day; the GPU rows use a lowered
  estimate to get run 49's thin margin.
- Larger vision models, audio input, Linux and macOS, release builds. The llama, runtime and api test suites were
  not run (as asked); the proof is the live requests above.
- User paths in the raw logs are written as `C:\Users\<user>\`.
