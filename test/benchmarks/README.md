# Benchmarks

One file per test type. Each file holds only the latest result for that type and links only to the run that
produced it. When a newer run replaces a result, the file is updated and the old numbers are removed.

| Test type | File | Latest run |
|---|---|---|
| MoE model larger than VRAM + RAM, experts streamed from the SSD | [ssd-streaming-moe.md](ssd-streaming-moe.md) | run 48 (generation), run 75 (prompt) |
| Same model, same placement, same settings as llama.cpp (GPU and GPU + RAM split) | [same-settings-gpu.md](same-settings-gpu.md) | runs 5, 9, 17 |
| CPU only | [cpu.md](cpu.md) | runs 15, 16, 54 |
| MTP (multi-token prediction) | [mtp.md](mtp.md) | run 24 |
| Planner: planned VRAM against VRAM taken | [planner.md](planner.md) | run 21 |
| Against Ollama | [ollama.md](ollama.md) | runs 10, 16 |

Speeds are llama.cpp's own timings in tokens per second (t/s); higher is better. Each run folder in
`test/cloud/runs/` has the commands, settings and raw logs.
