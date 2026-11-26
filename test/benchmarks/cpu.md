# CPU only

| Machine | Result | Run |
|---|---|---|
| 8-thread Windows laptop (Ryzen 7 7435HS), CPU backends built with clang and LLVM OpenMP | Qwen3-1.7B IQ4_XS pp512 / tg128: Cluaiz 129.45 / 25.03, llama-bench 136.16 / 26.15 t/s (about 5% behind), median of 8 rounds | [run 54](../cloud/runs/2026-11-23-run54-windows-llvm-cpu-backends/SUMMARY.md) |
| 1 vCPU VPS (AMD EPYC 9354P, 3.8 GB) | Bonsai 4B Q1_0 and Llama 3.2 3B Q4_K_M: prompt and generation equal to llama.cpp `b12428` (for example Llama 3.2 3B CPU: 21.17 / 8.16 against 21.80 / 7.60 t/s) | [run 15](../cloud/runs/2026-11-16-run15-vps-cpu/SUMMARY.md) |
| same VPS | Ternary-Bonsai Q2_0: 13-20% slower than llama.cpp | [run 16](../cloud/runs/2026-11-16-run16-vps-ollama/SUMMARY.md) |
