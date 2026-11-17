# Run 20: a MoE model larger than the free RAM on a 4 GB laptop GPU (2026-11-16)

| | |
|---|---|
| Machine | Laptop, AMD Ryzen 7 7435HS (8 cores), 23.7 GB RAM, RTX 3050 Laptop 4 GB, NVMe SSD (Micron 512 GB), Windows 11 |
| Cluaiz | local release build of `main` at `16cd9a5a` (the v0.1.0 tag), `cluaiz bench` and `cluaiz run --local -v` |
| Model | `bartowski/google_gemma-4-26B-A4B-it-GGUF:IQ4_XS`: MoE, 30 layers, 128 experts (8 active), model file 14.21 GB (13.23 GiB); with the vision projector and the MTP file the download is 15.72 GB (`cluaiz list`: 14.64 GiB); experts 11.52 GB |
| Records | [`results/show_after_measure.txt`](results/show_after_measure.txt): the plan after the first GPU load. The first bench and run outputs were read from the owner's terminal screenshots; the alternated comparisons are in [`results/ab-llama/`](results/ab-llama/) and [`results/ab-chat/`](results/ab-chat/) with their scripts (PowerShell 5.1 wraps stderr lines as `NativeCommandError`; the output itself is complete) |
| Other apps | Browser, IDE and desktop apps open: they held 1.6 GB of the GPU and 11-12 GB of RAM |

This repeats the README test of 2026-10-12 (16-18 t/s), whose raw log was not kept. The exact file of that
test was not recorded either.

## Results

| Run | Placement | Context | Read from the disk (mmap) | Generation t/s | Prompt t/s |
|---|---|---|---|---|---|
| `bench -p 512 -n 128 -r 3`, planner's choice | CPU only | 8,192 | 8.91 GB over the RAM limit (first try) / 3.07 GB | 9.03 ± 1.22 | 10.33 ± 3.22 |
| `bench -p 512 -n 128 -r 3 --n-gpu-layers all --n-cpu-moe 30` | MoE split: attention of 30/30 layers on the GPU, experts in RAM | 8,192 | not printed | **16.68 ± 0.35** | 119.33 ± 78.24 |
| `run --local`, a 300-word story, answer of 1,337 tokens with thinking | MoE split (user setting) | 16,384, KV F16 on the GPU | 1.37 GB | **10.56** | 4.07 (28 tokens) |
| Same, again, answer of 1,075 tokens | MoE split | 16,384 | 1.39 GB | **11.70** | 2.75 (28 tokens) |

Single runs on this machine vary by 25-30% with the same command, because the free RAM (and so the part of the
experts read from the disk) changes from run to run. The comparisons below were therefore alternated and
repeated.

## Against official llama.cpp (alternated, 2026-11-16)

Same model file and flags: all layers on the GPU, experts of 30 layers in RAM (`-ngl 99 -ncmoe 30` /
`--n-gpu-layers all --n-cpu-moe 30`), flash attention on, KV F16, 8 threads. Cluaiz: `main` at `16cd9a5a`;
llama.cpp `b12428`. Rounds alternate llama.cpp, Cluaiz, Cluaiz with `GGML_CUDA_NO_PINNED=1`.

| Round | llama-bench tg128 | Cluaiz bench tg128 (ctx 1024) | Cluaiz, no pinned host memory |
|---|---|---|---|
| 1 (first after other work) | 13.66 ± 3.14 | 14.35 ± 0.42 | 16.30 ± 0.19 |
| 2 | 15.52 ± 0.84 | 14.77 ± 0.61 | 14.58 ± 0.23 |
| 3 | 16.25 ± 0.42 | 15.06 ± 0.81 | 15.17 ± 0.93 |

Rounds 2-3: llama.cpp 15.9, Cluaiz 14.9 t/s, so Cluaiz generated about 6% slower (not explained yet). Prompt
(pp512) varied too much to compare: Cluaiz 95-119 ± 68-92, llama.cpp 133-155 ± 27-38.

## Chat answers against the bench (alternated, 2026-11-17)

Installed release v0.1.0, same flags, context 16,384, thinking off; `bench -p 0 -n 400 -r 2` against
`run --local` answering "Write a 300 word story about a robot." (362-371 tokens).

| Round | `bench` tg400 | `run` generation |
|---|---|---|
| 1 (experts not yet in RAM) | 7.28 ± 8.01 | 2.84 |
| 2 | 10.58 ± 4.89 | 10.77 |
| 3 | 12.05 ± 2.90 | 12.74 |

The chat path is as fast as the bench. A 400-token answer is slower than the 128-token bench (about 12 against
15 t/s): each token picks its own experts, and the longer the answer, the more of them come from the disk. The
first answer after a load, before the experts are in RAM, is far slower.

Tested and ruled out as causes of a gap with llama.cpp: thinking (`--think off`), Windows shared GPU memory
(the NVIDIA "Prefer No Sysmem Fallback" setting gave the same 11.4 t/s), pinned host memory (table above).

`llama-completion -no-cnv` sends the raw prompt without the model's chat template; with this chat model the
text degenerates, so its numbers are not used here.

## What the disk does here

The experts that do not fit in RAM stay in the model file and are read when a token needs them
(memory-mapped by the operating system; Cluaiz has no SSD prefetch yet). In the first CPU-only try, with RAM at
94% in use, Task Manager showed the NVMe 100% active but reading only about 52 MB/s at 3.9 ms per request:
small reads on page faults, with the page file on the same disk. The more of the model is read from the disk,
the slower each token gets; the README run of 2026-10-12 measured 7.4 t/s with about 3 GB of free RAM.

## The planner's choice

- The model had never been loaded on the GPU of this machine, so the planner kept the fixed 1 GB VRAM gap. With
  2.29 GB free it rejected the MoE split (estimated 2.73 GB) and chose CPU only (9.03 t/s).
- The forced MoE split loaded and took 2.46 GB of VRAM ("VRAM planned 3.10 GB, taken 2.46 GB"). Cluaiz keeps
  that measurement. After it, `cluaiz show` with no flags picks the MoE split by itself, with a Q8_0 KV cache
  to stay on the GPU at 16,384 tokens ([`results/show_after_measure.txt`](results/show_after_measure.txt)).
- So on a first load, when other apps hold VRAM, the planner can leave the GPU unused. This is a known issue to
  fix: try a smaller gap, a smaller KV cache and a partial MoE split before CPU only.

## Not measured

- The disk read rate per run (the per-second logger of the comparison script recorded nothing). Task Manager
  showed 98-120 MB/s of reads during both llama.cpp and Cluaiz chat runs.
- The cause of the 6% generation gap with llama-bench.
- With user settings (planner bypassed) at 16,384 tokens, Cluaiz's estimate was 0.25-0.33 GB over the usable
  VRAM and the plan only warned; the load still ran. Not related to the speed (see above), but a planner issue.
