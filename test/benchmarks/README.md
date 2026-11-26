<!-- name: test/benchmarks
description: Every test of the Engine written up, one file per test type, latest results only, with links to the raw logs. -->

# Benchmarks and test results

Every test of the Engine is written up here, one file per test type, with only the latest results: setup,
method, commands, every number, charts, what it proves and what it does not, and at the end the link to the run
folder with the raw logs and scripts. Run folders (`test/cloud/runs/`) hold only logs, raw results and scripts.
When a newer run covers the same case, the old numbers and the old run folder are deleted. Images and GIFs go in
[`assets/`](assets/).

| Test type | File |
|---|---|
| MoE models larger than VRAM + RAM: SSD streaming, MoE cache, expert prefetch | [ssd-streaming-moe.md](ssd-streaming-moe.md) |
| Speed against official llama.cpp with the same settings | [speed-vs-llamacpp.md](speed-vs-llamacpp.md) |
| Against Ollama, LM Studio and llama.cpp as servers, and next to a website | [vs-ollama-and-load.md](vs-ollama-and-load.md) |
| Planner and VRAM | [planner-and-vram.md](planner-and-vram.md) |
| MTP | [mtp.md](mtp.md) |
| Release builds on every platform | [release-and-platforms.md](release-and-platforms.md) |
| Tool calls, tool loops, JSON schema, small models | [tools-and-agents.md](tools-and-agents.md) |
| OpenAI and Ollama API compatibility | [api-compatibility.md](api-compatibility.md) |
| Thinking | [thinking.md](thinking.md) |
| OS sandbox | [os-sandbox.md](os-sandbox.md) |
| Workflows | [workflows.md](workflows.md) |

Speeds are llama.cpp's own timings in tokens per second (t/s); higher is better.
