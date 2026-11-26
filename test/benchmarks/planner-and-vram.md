# Planner: VRAM planned against VRAM taken, vision projector VRAM

## Answer

- **Planner (run 21, L4):** VRAM planned against VRAM taken within 0.09 GB for 20 of 22 placements, 6 release
  models and 16 architectures. A model that cannot fit is refused with the amount missing.
- **Vision projector VRAM (run 50, 4 GB GPU):** before the fix the 3rd image request failed with HTTP 500 (704 MiB
  estimate); after it 15 / 15 answered. Text before image: 0 / 7 correct before, 7 / 7 after (CPU projector).

Below: every current run of this test type, newest first, in full.

<a id="run-50"></a>

## Run 50 (2026-11-23)

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

#### Bug 1: image requests after the first get HTTP 500 on a GPU projector

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

| Estimate | Raw |
|---|---|
| 528, 560, 592, 624 MiB | `raw/sweep/before_ws5*.txt`, `before_ws6*.txt` |
| 704 MiB | `raw/sweep/before_ws704_image_first.txt` |
| 736, 768, 800 MiB | `raw/sweep/before_ws7*.txt`, `before_ws800*.txt` |

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

#### Bug 2: with the text part before the image, the model says it cannot see images

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

#### What this run does not prove

- The reclaim path (a GPU projector moved to the CPU after a failed VRAM check): not hit.
- A large image after small ones on a GPU projector (growth is counted after a request ran).
- The 1 GiB default estimate on the GPU: the projector went to the CPU on this day; the GPU rows use a lowered
  estimate to get run 49's thin margin.
- Larger vision models, audio input, Linux and macOS, release builds. The llama, runtime and api test suites were
  not run (as asked); the proof is the live requests above.
- User paths in the raw logs are written as `C:\Users\<user>\`.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-23-run50-vision-vram/`](../cloud/runs/2026-11-23-run50-vision-vram/)

<a id="run-21"></a>

## Run 21 (2026-11-18)

| | |
|---|---|
| Machine | Google Cloud `g2-standard-8` (standard, us-central1-c): NVIDIA L4 23,034 MiB, 8 vCPU, 31 GB RAM, Ubuntu, CUDA 13.2 |
| Cluaiz | `main` at `7f489c06`, built with `remote/linux_build.sh - 89`. Runs 21b and 21c add the two fixes of this run (below) on top |
| Scripts | [`remote/run21_planner.sh`](../cloud/remote/run21_planner.sh), [`remote/vram_hog.py`](../cloud/remote/vram_hog.py), [`remote/multiturn_cache.py`](../cloud/remote/multiturn_cache.py); 21c: [`run21c.sh`](../cloud/runs/2026-11-18-run21-l4-planner/run21c.sh) |
| Records | [`results-run21/`](../cloud/runs/2026-11-18-run21-l4-planner/results-run21/) (21 and 21b), [`results-run21c/`](../cloud/runs/2026-11-18-run21-l4-planner/results-run21c/), [`results-run21-first/`](../cloud/runs/2026-11-18-run21-l4-planner/results-run21-first/) (outputs before the fixes), `run21*.log` |
| Home | isolated `CLUAIZ_HOME` per run (`~/ch-run21`, `~/ch-run21c`), port 8011 |

The first attempt ran on a Spot VM (`cluaiz-l4b`). Google reclaimed it twice, and the run moved to the
standard VM. Qwen3-30B-A3B and gpt-oss-20b were linked from an earlier run's home, not pulled again.

#### Results: VRAM planned against taken

"Planned" is the planner's corrected estimate. A buffer is kept free beside it: 0.15 GB reserve + 0.30 GB
for the GPU pool for a model not measured yet, from the log line `VRAM planned X, taken Y`.

##### Release models (`bench --placements -p 512 -n 128`)

| Model | Placement | Planned | Taken | pp512 t/s | tg128 t/s |
|---|---|---|---|---|---|
| Llama 3.2 3B Q4_K_M | Full GPU | 3.90 | 3.94 | 7,593 | 109.6 |
| Gemma 3 12B Q4_K_M, **before the fix** | Full GPU | 12.93 | 8.85 | 2,014 | 31.7 |
| Gemma 3 12B, **after both fixes** (21c, fresh home) | Full GPU | 8.81 | 8.85 | 2,042 | 31.7 |
| Mistral Small 3.2 24B Q4_K_M | Full GPU | 15.79 | 15.84 | 1,066 | 17.5 |
| Qwen3-30B-A3B Q4_K_M (MoE) | Full GPU | 18.93 | 18.98 | 3,275 | 102.8 |
| gpt-oss-20b MXFP4 | Full GPU | 11.50 | 11.55 | 3,614 | 83.1 |
| Gemma 4 31B Q4_K_M | Full GPU | 20.04 | 20.06 | 765 | 13.0 |

Gemma 4 31B gets 16,384 tokens of context with all layers on the GPU (KV F16 2.42 GB, `show_gemma-4-31b.txt`).
Before the sliding-window load fix of `f52044c5` it got 4,096.

##### Architecture sweep (`bench -p 128 -n 32 -r 1`, one load each, Q4_K_M)

| Architecture | Model | Planned | Taken | Taken - planned |
|---|---|---|---|---|
| llama | Llama 3.2 1B | 1.51 | 1.55 | +0.04 |
| gemma2 (sliding, default pattern) | Gemma 2 2B | 2.74 | 2.77 | +0.03 |
| gemma3 (sliding, default pattern) | Gemma 3 1B | 1.35 | 1.39 | +0.04 |
| gemma3n (shared KV) | Gemma 3n E2B | 2.09 | 2.12 | +0.03 |
| qwen3 | Qwen3 1.7B | 3.09 | 3.13 | +0.04 |
| qwen35 (hybrid) | Qwen3.5 2B | 1.88 | 1.93 | +0.05 |
| phi3 | Phi-4-mini | 4.72 | 4.77 | +0.05 |
| mistral3 | Ministral 3 3B | 3.90 | 3.94 | +0.04 |
| olmo2 | OLMo 2 1B | 1.47 | 1.51 | +0.04 |
| granitehybrid (Mamba hybrid) | Granite 4.0-H-Tiny | 4.32 | 4.41 | +0.09 |
| lfm2 (hybrid) | LFM2 1.2B | 1.07 | 1.12 | +0.05 |
| deepseek2 (MLA, MoE) | DeepSeek-Coder-V2-Lite | 14.02 | 14.47 | **+0.45** |
| deepseek2 (MLA, vision) | Kimi-VL-A3B-Thinking | 10.81 | 10.53 | -0.28 |
| deepseek2 (MLA) | Moonlight-16B-A3B | 10.36 | 10.26 | -0.10 |
| GLM-4.7-Flash | GLM-4.7-Flash | 18.66 | 18.08 | -0.58 |
| kimi-linear (hybrid, MoE split) | Kimi-Linear-48B-A3B | 21.08 | 21.82 | **+0.74** |

All 16 loaded and ran; none failed. 14 are within 0.09 GB of the plan.

Two are under the plan:
- **DeepSeek-Coder-V2-Lite:** +0.45 GB, exactly the buffer kept for an unmeasured model.
- **Kimi-Linear 48B:** +0.74 GB, more than that buffer. It loaded because the GPU was otherwise empty.

The pool gap learned from such a first load covers the next load. These two layouts (MLA, Kimi-Linear's
linear-attention state) are the next estimate work. The MLA models of the Kimi and GLM families are
over-estimated, which is safe but wastes up to 0.58 GB.

#### Bugs found and fixed in this run

1. **Sliding-window layers counted as full attention when the file lists none.**
   - Gemma 3 files carry `sliding_window` but no `sliding_window_pattern` array.
   - llama.cpp applies a per-architecture period (`load_swa_pattern(ml, 6)` in `src/models/gemma3.cpp`; 2 for gemma2 and gpt-oss, 4, 5 or 8 for others).
   - The prober now does the same (`engine/models/src/prober/gguf.rs`, `default_swa_pattern`, `swa_pattern`, with a unit test).
   - Gemma 3 12B KV at 16,384 tokens: 6.00 GB before, the llama.cpp layout after.
2. **Learned margins measured against the old estimate were applied to the new one.**
   - A pool gap of -3.89 GB, learned while the estimate was 4 GB too high, made the first load after the fix under-plan by 0.64 GB (21b: planned 8.39, taken 9.03).
   - The correction and pool-gap keys now carry a version (`proj3:`, `pool2:` in `engine/plan/src/margin.rs`), so older ones are dropped, as the code already did for `proj2:`.
   - 21c in a fresh home: planned 8.81, taken 8.85.

Unit tests after both fixes: `cluaiz-models` 43 passed, `cluaiz-plan` 54 passed (release).

#### Step-down when another process takes VRAM during a load

- **21:** invalid. The other process took the VRAM 6 s after the start and gave it back after 60 s, but
  reading Qwen3-30B-A3B from the disk took 104 s, so the load never saw it.
- **21c:** `vram_hog.py load 1.5 180` waits until the load has taken 1 GB, then takes all but 1.5 GB of
  what is free.
  - Log: `17:16:41 free 4.60 GB, took 3.10 GB`.
  - The planned full-GPU load failed ("Context Init Failure").
  - The Engine retried one placement smaller: MoE split, experts of 48/48 layers in RAM. That load
    worked and the bench ran (pp64 33.0, tg16 12.0 t/s). No crash.
  - The retry line names the same `-1 GPU layers, 16384 tokens`; the change is in `n_cpu_moe`
    (0 -> 48), which the message does not print.

#### Multi-turn chat without a session id (Gemma 3 12B, `serve`)

| Turn | Answer | Prompt tokens | Reused from the cache | Processed |
|---|---|---|---|---|
| 1 | OK | 1,828 | 0 | 1,827 |
| 2 | BLUE-42 | 1,854 | 1,828 | 22 |
| 3 | EULB | 1,875 | 1,854 | 18 |

PASS: later turns reuse the whole earlier prompt, and the fact from turn 1 is kept.

#### Not done

- No A/B against official llama.cpp on the L4 (done on the laptop on 2026-11-18, GPU parity).
- The planner estimate for MLA and Kimi-Linear is not fixed yet (see the sweep).
- The retry message does not print the changed `n_cpu_moe`.

Logs, raw results and scripts: [`test/cloud/runs/2026-11-18-run21-l4-planner/`](../cloud/runs/2026-11-18-run21-l4-planner/)

## All logs

| Run | Folder |
|---|---|
| 50 | [`2026-11-23-run50-vision-vram`](../cloud/runs/2026-11-23-run50-vision-vram/) |
| 21 | [`2026-11-18-run21-l4-planner`](../cloud/runs/2026-11-18-run21-l4-planner/) |
