# Run 9: Cluaiz vs llama.cpp on an NVIDIA L4 (2026-11-14)

The run 5 comparison (Tesla T4) repeated on the run 8 VM: `test/cloud/remote/regression_all.sh` with
`test/engine_regression/run.py`.

| | |
|---|---|
| VM | Google Cloud `g2-standard-8`, NVIDIA L4 22.5 GB (compute 8.9), Xeon 2.2 GHz 8 vCPU, 31 GiB RAM |
| OS / driver | Debian 13, driver 620.70.06; CUDA toolkit 13.2 (Google's `cuda_installer.pyz`) only to build llama-bench |
| Cluaiz | release v0.1.0 (`14ae569f`) installed by `install.sh`, CUDA 13 packs, through its chat API |
| llama.cpp | `llama-bench` built on the VM from the same fork commit (`8f9842cd1`), CUDA, `sm_89`, given the placement Cluaiz chose |
| Method | prompt ~544 tokens, 64 generated tokens, 3 runs per model and mode (first is cold) |

Reports: `results-regression/RESULTS_l4_<model>.md`; planner and server logs beside them.

## Results (tokens per second, prompt / generation)

| Model | Mode | Placement | Cluaiz | llama.cpp | Generation ratio |
|---|---|---|---|---|---|
| Qwen3 4B Q4_K_M | auto | full GPU | 4483 / 82.9 | 4560 / 82.9 | 1.00 |
| | gpu | full GPU | 4735 / 82.6 | 4518 / 82.9 | 1.00 |
| | split | half the blocks on GPU | **832** / 14.7 | 625 / 15.3 | 0.96 |
| Gemma 3 12B Q4_K_M | auto | full GPU | 1477 / 30.4 | 1688 / 30.5 | 1.00 |
| | gpu | full GPU | 1497 / 30.4 | 1666 / 30.4 | 1.00 |
| | split | half | **294** / 6.3 | 240 / 6.3 | 1.00 |
| Mistral Small 3.2 24B Q4_K_M | auto | full GPU | 922 / 17.1 | 972 / 17.1 | 1.00 |
| | gpu | full GPU | 916 / 17.1 | 968 / 17.1 | 1.00 |
| | split | half | 153 / 3.4 | **204** / 3.4 | 1.00 |
| Gemma 4 31B Q4_K_M (17.1 GB) | auto | GPU + RAM, 57 of 60 blocks on GPU | 443 / 8.8 | 450 / 8.8 | 1.00 |
| | gpu | full GPU | 586 / **12.4** | 648 / 12.4 | 1.00 |
| | split | half | 116 / 2.4 | 118 / 2.4 | 1.01 |
| Gemma 4 31B Q8_0 (30.4 GB) | auto | GPU + RAM | 87 / 2.0 | 88 / 2.0 | 1.00 |
| | gpu | forced full GPU | failed (does not fit 22.5 GB; see 3) | — | — |
| | split | half | **71** / 1.7 | 60 / 1.7 | 1.00 |

14 of 15 passed.

## Findings

1. **Generation equals llama.cpp in every test** (ratio 0.96–1.01).
2. **Prompt:** fully on the GPU llama.cpp is up to 11% faster (Gemma 3 12B, Gemma 4 31B); split, Cluaiz is
   up to 1.3x faster (Qwen3 4B, Gemma 3 12B, Gemma 4 Q8) except Mistral (llama.cpp 1.3x faster).
3. **Gemma 4 31B Q4 on 22.5 GB: the planner chose GPU + RAM (8.8 t/s) though the full-GPU load fits**
   (forced, it took 21.3 GB and wrote 12.4 t/s, 1.4x faster). The planner's estimate (18.70 GB for 57 blocks)
   leaves the last blocks in RAM.
4. Forcing the 30.4 GB Q8 model fully on the GPU fails as expected; the server log gives the reason (CUDA
   buffer of 32.6 GB, CPU fallback refused because the model is larger than free RAM), but the chat API
   answers HTTP 500 without that reason.
