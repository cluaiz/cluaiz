# Cluaiz vs llama.cpp on a cloud Tesla T4: test results (2026-10-28)

> **Saar (Hinglish).** Laptop wala test (`test/engine_regression/RESULTS.md`) ab Google Cloud ke Tesla T4
> (16 GB) par, release v0.1.0 ko ek command se install karke. **Jawab likhne ki speed (generation) har test mein
> llama.cpp ke barabar** (18 mein se 18, -6% se +13%). Prompt padhne mein mila-jula: model GPU + RAM mein bata
> ho aur prompt 512 ke thoda upar ho (jaise 535) to Cluaiz 1.3x–1.5x tez; baaki jagah release mein llama.cpp
> 7–22% aage tha (prompt ke tukde, aur Gemma ka 4-token checkpoint). Do fix (`6120d017`, `4228752a`) ke baad
> T4 par prompt bhi barabar: poore GPU par 2–5% ke andar, split par barabar ya Cluaiz 1.6x tak tez. Gemma 4 31B Q4 (17 GB) aur Q8 (32 GB)
> dono chale; planner ne khud GPU + RAM mein baanta aur wahi sabse tez mode nikla. SSD streaming (15 GB RAM) baaki.

## Who is who in this file

| Name in the tables | What it is | How it was run |
|---|---|---|
| **Cluaiz** | Release v0.1.0 (`7700bba`), installed with `install.sh` (CUDA 12 pack), `cluaiz serve`, called through its chat API (`/v1/chat/completions`) | Planner chooses the placement, prompt is split evenly, tokens are sampled and streamed like a real chat |
| **llama.cpp** | Plain llama.cpp, nothing from Cluaiz: its own `llama-bench` tool | Built on the VM from the **same** llama.cpp source as the release (fork commit `8f9842cd1`), CUDA, for the T4 (`sm_75`), given the **same** placement Cluaiz chose |

Same VM, same model file, same placement, same threads / batch / KV type, so the only difference is Cluaiz's own
code around llama.cpp. Speeds are in tokens per second (t/s): higher is better.

## Short answer

| | Cluaiz vs llama.cpp |
|---|---|
| Generation speed (writing the answer) | **Same** in all 18 tests (-6% to +13%; the T4 moves a few percent between runs) |
| Prompt, release v0.1.0 | Split, prompt just over 512 tokens: **Cluaiz 1.3x–1.5x faster**; all other cases: **llama.cpp 7–22% faster** on the T4 |
| Prompt, with the two fixes (`6120d017`, `4228752a`) | **Same** fully on the GPU (llama.cpp +2–5%), **same or Cluaiz up to 1.6x faster** when split |
| Prompt, CPU only (measured fairly, `-dev none`) | **Same** (4B: 27 / 29, 12B: 10 / 10, 24B: 5 / 5, 31B: 4 / 4) |
| Planner's choice (auto) | **The fastest mode** for every model that does not fit the GPU |
| Gemma 4 31B (17 GB and 32 GB files, 16 GB GPU) | **Cluaiz runs both**, generation equal to llama.cpp (Q4 3.2 / 3.1, Q8 1.3 / 1.3) |
| Tests passed (Cluaiz checks: answer, no leaked tokens) | 18 of 18 (Q8 CPU only skipped: does not fit 29 GB RAM) |

## Machine

| | |
|---|---|
| VM | Google Cloud `n1-standard-8` + 1× NVIDIA Tesla T4, Spot, us-east1-c (stopped by Google twice during the run, restarted) |
| CPU | Intel Xeon @ 2.30 GHz, 4 cores / 8 threads, AVX2 (Cluaiz and llama-bench use 4 threads) |
| RAM | 29.4 GB |
| GPU | Tesla T4, 15.0 GB VRAM, compute 7.5, ~320 GB/s memory |
| Disk | 200 GB balanced persistent disk, read ~206 MB/s (`hdparm -t`) |
| OS | Ubuntu 24.04, NVIDIA driver 580.178.04 (CUDA 13.0) |
| llama.cpp source | cluaiz fork `8f9842cd1`, used by both Cluaiz and the llama.cpp reference |

## Generation speed (t/s): Cluaiz vs llama.cpp

| Model (file size) | Mode | Placement | **Cluaiz** | **llama.cpp** | Result |
|---|---|---|---|---|---|
| Qwen3 4B Q4_K_M (2.33 GB) | auto | full GPU | 65.5 | 62.3 | Cluaiz +5% |
| | gpu | full GPU | 62.9 | 60.1 | Cluaiz +5% |
| | split | half the blocks on GPU (`ngl 19`) | 14.5 | 13.9 | same (+4%) |
| | cpu | CPU only | 7.7 | 7.6 | same |
| Gemma 3 12B Q4_K_M (6.80 GB) | auto | full GPU | 17.0 | 15.7 | Cluaiz +9% |
| | gpu | full GPU | 14.4 | 12.7 | Cluaiz +13% |
| | split | half the blocks on GPU (`ngl 25`) | 5.5 | 5.4 | same |
| | cpu | CPU only | 3.1 | 3.0 | same |
| Mistral Small 3.2 24B Q4_K_M (13.35 GB) | auto | full GPU | 8.0 | 8.2 | same |
| | gpu | full GPU | 8.3 | 8.2 | same |
| | split | half the blocks on GPU (`ngl 21`) | 3.1 | 3.1 | same |
| | cpu | CPU only | 1.8 | 1.8 | same |
| Gemma 4 31B Q4_K_M (17.07 GB) | auto | **41 of 60 blocks on GPU (planner)** | **3.2** | 3.1 | same |
| | gpu | forced full GPU: does not fit, weights went to RAM (`ngl 0`)** | 1.2 | 1.3 | same (-6%) |
| | split | half the blocks on GPU (`ngl 31`) | 2.3 | 2.3 | same |
| | cpu | CPU only | 1.3 | 1.3 | same |
| Gemma 4 31B Q8_0 (30.39 GB) | auto | **23 of 60 blocks on GPU (planner, `ngl 24`)** | **1.3** | 1.3 | same |
| | split | half asked (`ngl 31`, 17.9 GB): did not fit, load fallback `ngl 15` | 1.1 | 1.1 | same |
| | cpu | file 30.4 GB > free RAM 27.8 GB: skipped | - | - | |

The T4's speed moves a few percent between runs (clock, temperature): Gemma 3 12B on full GPU gave 20–21 t/s in
an earlier round and 14–17 t/s in this one, for both Cluaiz and llama.cpp. Compare the two columns of one row,
not rows of different rounds.

\*\* Forcing `n_gpu_layers -1` for a model that cannot fit: the user's setting goes to llama.cpp as it is
(the planner is bypassed). llama.cpp could not allocate 18.3 GB on the GPU, so the engine's load fallback put the
weights in RAM (`ngl 0`) and kept the GPU for the prompt's matrix work: 1.2 t/s. The planner's own choice for
the same model (auto) gives 3.2 t/s. User settings stay as they are by design; only the load fallback acts, when
llama.cpp itself fails.

## Prompt speed (t/s): Cluaiz vs llama.cpp

| Model | Prompt tokens | Mode | **Cluaiz** | **llama.cpp** | Result |
|---|---|---|---|---|---|
| Qwen3 4B | 535 (512 + 23) | auto (full GPU) | 1758 | 1882 | llama.cpp +7% |
| | | gpu (full GPU) | 1722 | 1855 | llama.cpp +8% |
| | | split | 584 | 380 | **Cluaiz 1.5x** |
| | | cpu | 27 | 29* | llama.cpp +8%* |
| Gemma 3 12B | 537 (512 + 25) | auto (full GPU) | 572 | 649 | llama.cpp +13% |
| | | gpu (full GPU) | 536 | 596 | llama.cpp +11% |
| | | split | 185 | 139 | **Cluaiz 1.3x** |
| | | cpu | 10 | 10* | same* |
| Mistral 24B | 527 (Cluaiz) / 1085 (llama.cpp) | auto (full GPU) | 315 (1085 tokens: 352) | 340 | llama.cpp +8% (1085 tokens: same) |
| | | gpu (full GPU) | 316 (1085 tokens: 343) | 350 | llama.cpp +11% (1085 tokens: same) |
| | | split | 119 (1085 tokens: 150) | 143 | llama.cpp +20% (1085 tokens: same) |
| | | cpu | 5 | 5* | same* |
| Gemma 4 31B Q4 | 544 (512 + 32) | auto (split, 41/60) | 96 | 117 | llama.cpp +22% |
| | | gpu (forced, weights in RAM) | 45 | 4** | not comparable** |
| | | split (half) | 75 | 91 | llama.cpp +21% |
| | | cpu | 4 | 4 | **same** (fair: `-dev none`) |
| Gemma 4 31B Q8 | 544 (512 + 32) | auto (split, 23/60) | 45 | 51 | llama.cpp +13% |
| | | split (`ngl 15`) | 38 | 42 | llama.cpp +10% |

Cluaiz numbers are the average of the warm requests (2 and 3). llama-bench is given the length of the first
request; the warm ones are a few tokens shorter (527–539). Mistral is the exception: its first request (thinking
on) was 1085 tokens and the warm ones only 527, so llama-bench measured 1085; Cluaiz's own first request at the
same 1085 tokens is shown in brackets and matches llama.cpp.

**\* CPU only, Qwen3 / Gemma 3 / Mistral.** Cluaiz gives a CPU-only model no GPU at all (`set_cpu_only`: an
empty device list). `llama-bench -ngl 0` keeps the weights in RAM but still sends the prompt's matrix work to
the GPU, so its first numbers (209 / 79 / 99) were not CPU numbers. `run.py` now passes `-dev none` for
CPU-only tests (fixed during this run). The rows above are llama-bench run again with `-dev none`
(`cpu_devnone.log`; generation there 7.9 / 3.1 / 1.7, the same as Cluaiz). Gemma 4 31B was measured that way in
the run itself: 4 / 4.

**\*\* Forced GPU, Gemma 4 31B Q4:** Cluaiz reported `CpuOnly ngl 0`, so `run.py` gave llama-bench `-dev none`,
but Cluaiz had kept the GPU for the prompt (46 t/s). Not a like-for-like pair.

**What decides who is faster on the prompt: how the prompt is cut.** llama.cpp cuts it into chunks of 512
tokens. Cluaiz cuts it into equal chunks of at most 512 (535 → 268 + 267, 544 → 272 + 272).

- When llama.cpp's last chunk is **below 32 tokens** (535 = 512 + 23), the layers in RAM compute that chunk on
  the CPU, which is slow. Cluaiz's equal chunks avoid it: **Cluaiz 1.3x–1.5x faster on split models**. This is
  the case the laptop test measured.
- When the last chunk is **32 or more** (544 = 512 + 32), both take the fast path, and on the T4 one chunk of
  512 is faster than two of 272: **llama.cpp 7–22% faster**, on split and on full GPU.
- Mistral at 1085 tokens (362 × 3 against 512 + 512 + 61) came out the same; at 527 tokens (264 + 263) Cluaiz
  was 8–20% behind llama.cpp's 1085, in line with the case above.

**Fix 1 (commit `6120d017`):** chunks of 512 like llama.cpp. Only when weights stay in RAM
while a GPU is used (split, MoE experts in RAM) and the last chunk would be below 32 tokens are the last two
chunks made equal (535 → 268 + 267). Fully on the GPU or with no GPU: always llama.cpp's cut (535 → 512 + 23,
544 → 512 + 32, 1085 → 512 + 512 + 61). That keeps Cluaiz's win on split models and removes the loss elsewhere.

**Fix 2 (commit `4228752a`):** Gemma 3 and Gemma 4 are sliding-window models. For the next turn the engine
saves a checkpoint 4 tokens before the prompt end (llama-server's offset), so those 4 tokens are a separate
llama_decode call. With weights in RAM that 4-token chunk runs on the CPU, about 1 s per prompt on Gemma 4 31B.
Fix 1 alone therefore did not move Gemma 4 (94 against 115). With weights in RAM the checkpoint now keeps 32
tokens after it (544 → 512 + 32, exactly llama.cpp's cut); fully on the GPU it stays 4. The next turn processes
up to 28 tokens more, a few milliseconds.

## After the two prompt fixes

Engine built on the VM from `main` with both fixes (`CLUAIZ_BIN`, same CUDA 12 / `sm_75` build as llama-bench),
same models and `run.py`, auto and split. Reports: `RESULTS_fix_*.md` (fix 1), `RESULTS_fix2_*.md` (fix 1 + 2,
Gemma 3 / 4).

| Model | Mode | Prompt t/s, release v0.1.0 (Cluaiz / llama.cpp) | Prompt t/s, with the fixes (Cluaiz / llama.cpp) | Generation with the fixes |
|---|---|---|---|---|
| Qwen3 4B | auto (full GPU) | 1758 / 1882 (llama.cpp +7%) | **1935 / 1978 (+2%)** | 68.0 / 66.9 |
| | split | 584 / 380 (Cluaiz 1.5x) | **593 / 393 (Cluaiz 1.5x)** | 14.3 / 13.3 |
| Gemma 3 12B | auto (full GPU) | 572 / 649 (llama.cpp +13%) | **690 / 725 (+5%)** | 24.6 / 22.6 |
| | split | 185 / 139 (Cluaiz 1.3x) | **228 / 141 (Cluaiz 1.6x)** | 5.6 / 5.4 |
| Mistral 24B | auto (full GPU) | 315 / 340 (llama.cpp +8%) | **372 / 385 (+3%)** | 10.6 / 10.0 |
| | split | 119 / 143 | 119 / 153 (527 tokens against 1085; first request at 1085: 154 / 153) | 3.1 / 3.1 |
| Gemma 4 31B Q4 | auto (41/60 on GPU) | 96 / 117 (llama.cpp +22%) | **116 / 117 (same)** | 3.2 / 3.1 |
| | split | 75 / 91 (llama.cpp +21%) | **90 / 56** (llama.cpp measured 87–91 in the two runs before) | 2.3 / 2.3 |
| Gemma 4 31B Q8 | auto (23/60 on GPU) | 45 / 51 (llama.cpp +13%) | **50 / 51 (same)** | 1.4 / 1.4 |
| | split | 38 / 42 (llama.cpp +10%) | **43 / 44 (same)** | 1.2 / 1.2 |

- The prompt gap on the T4 is closed: same within 2–5% fully on the GPU, same or Cluaiz faster when split.
  The split win of the laptop test is kept and grew for Gemma 3 (1.6x).
- Generation is unchanged and equal to llama.cpp (0.99x to 1.09x).
- The T4 itself moves between runs (Gemma 3 12B full GPU generation: 14–25 t/s across the day, both
  engines alike); compare the two numbers of one cell.

## Planner decisions (auto mode)

| Model | Planner chose | Why (from the server log) |
|---|---|---|
| Qwen3 4B | Full GPU, context 40960, KV F16 | weights 2.32 + KV 5.62 + buffers 0.31 = 8.26 GB of 13.56 GB usable VRAM |
| Gemma 3 12B | Full GPU, context 17408, KV F16 | weights 6.79 + KV 6.38 + buffers 0.54 = 13.71 GB of 13.76 GB usable VRAM |
| Mistral 24B | Full GPU, context 4096, KV Q8_0 | weights 12.99 + KV 0.33 + buffers 0.29 = 13.62 GB of 13.77 GB usable VRAM |
| Gemma 4 31B Q4 | GPU + RAM split, 41 of 60 blocks + output head on GPU (13.97 GB), 19 blocks in RAM (6.20 GB), context 4096, KV Q8_0 | full GPU needs 19.43 GB; CPU only fits (18.34 GB) but is slower |
| Gemma 4 31B Q8 | GPU + RAM split, 22–23 of 60 blocks + output head on GPU (13.9–14.4 GB), 37–38 blocks in RAM, context 4096, KV Q8_0 | full GPU needs 32.76 GB; CPU only needs 31.67 GB, 6.51 GB more RAM than allowed |

The planner fills the GPU with context when the model is small (4B: 40960 tokens), shrinks the context and the
KV type to keep a 13 GB model fully on the GPU (Mistral: 4096, Q8_0), and keeps the context at the 4096 floor
when the model has to be split, so every byte of VRAM goes to weights.

For Gemma 4 31B Q4 the planner's choice was the fastest of the four modes: 3.2 t/s against 2.3 (half on GPU),
1.3 (CPU only) and 1.2 (forced full GPU).

## Cloud T4 vs the laptop (Cluaiz, same test)

| Model | Mode | Laptop RTX 3050 4 GB (gen / prompt) | Cloud T4 16 GB (gen / prompt) |
|---|---|---|---|
| Qwen3 4B Q4_K_M | full GPU | 39.3 / 1863 | 65.5 / 1758 |
| | split | 15.9 / 810 | 14.5 / 584 |

On full GPU the T4 writes faster (more memory bandwidth); the prompt is about the same. Split is slower on the
cloud VM: its 4 CPU cores and RAM are slower than the laptop's Ryzen 7 (8 cores).

## Settings used

**Cluaiz** (defaults, as installed):

| Setting | Value |
|---|---|
| n_ctx | Auto, at least 4096 (planner) |
| KV cache | Auto: F16 when it fits, Q8_0 when split (per row in the detail reports) |
| flash attention | Auto |
| op_offload | on |
| threads | 4 (physical cores) |
| batch / ubatch | 512 / 512 |
| mmap | on |

**Modes** (what was sent in the request's `llama` params): auto = nothing (planner decides); gpu =
`n_gpu_layers: -1`; split = half of the blocks on the GPU; cpu = `n_gpu_layers: 0`.

**llama.cpp** (`llama-bench`): the same `-ngl` / `-ncmoe` / KV type / flash attention / op offload / threads /
batch / ubatch / mmap that Cluaiz reported for that test, `-r 2`. Prompt: `-p <tokens> -n 0`. Generation:
`-p 0 -n 64 -d <tokens>`. Exact commands per test are in the detail reports.

**Requests to Cluaiz:** prompt ~535 tokens with a random number in front (no cache reuse), `max_tokens` 64,
3 runs per test. Run 1 is the cold run (also checks thinking and leaks); the speeds are the average of runs 2–3
with thinking off.

## Abhi baaki (still running)

- SSD streaming: the same VM with 15 GB RAM, Gemma 4 31B Q8 (32 GB file > 15 GB RAM + 15 GB VRAM).

## Not covered by this test

- Only NVIDIA T4 with the CUDA 12 pack; no AMD / Intel / Vulkan, no Mac, no Windows in this run.
- Long chats (8k+ tokens).
- First-call slowness of the CUDA 12 pack on the T4 (the driver compiles the GPU code once, 80–95 s) is not
  in these numbers; see `../2026-10-28-run4b-linux-t4-release-cuda12/SUMMARY.md`.

## Files

- `RESULTS_ram29_<model>.md`: `run.py`'s report per model, with every setting per test and the exact
  llama-bench command.
- Scripts: `test/cloud/remote/regression_tests.sh` (one model), `test/cloud/remote/regression_all.sh` (one
  model at a time, picks up after a Spot stop), models in `test/cloud/remote/models_regression.txt`.

## Run it again

```
bash test/cloud/vm.sh create cluaiz-t4-reg ...          # T4 VM: see the usage at the top of test/cloud/vm.sh
bash test/cloud/vm.sh put cluaiz-t4-reg test/cloud/remote/regression_tests.sh rt/
bash test/cloud/vm.sh put cluaiz-t4-reg test/cloud/remote/regression_all.sh rt/
bash test/cloud/vm.sh put cluaiz-t4-reg test/cloud/remote/models_regression.txt rt/
# on the VM:
~/rt/regression_all.sh 7700bbaa 75 ~/rt/models_regression.txt ram29 qwen3-4b,gemma-3-12b,mistral,gemma-4-31b-it-gguf-q4 \
    --modes auto,gpu,split,cpu --runs 3
```
