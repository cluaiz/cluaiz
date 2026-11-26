# Against Ollama

| Machine | Result | Run |
|---|---|---|
| GCP L4, Ollama 0.40.16, same GGUF bytes | Generation equal where the placement is equal (Qwen3 4B 82.9 vs 81.9, Gemma 3 12B 30.4 vs 30.6 t/s). Gemma 4 31B: Ollama kept it fully on the GPU (12.8 t/s), Cluaiz's planner split it (8.9 t/s) | [run 10](../cloud/runs/2026-11-15-run10-ollama/SUMMARY.md) |
| 1 vCPU / 3.8 GB VPS, no GPU | Ollama generated 14-23% slower than Cluaiz with the same settings and could not load Llama 3.2 3B (killed for memory) | [run 16](../cloud/runs/2026-11-16-run16-vps-ollama/SUMMARY.md) |
