<p align="center">
  <img alt="Cluaiz" src="assets/logo.webp" width="30%">
</p>

<h3 align="center">
The AI engine where models, voice, vision, memory, tools, agents and workflows work as one, under your control, in everything you build.</h3>

</br>
<p align="center">
  <img src="https://img.shields.io/badge/Status-Pre--release_v0.1.0-orange?style=for-the-badge" alt="Status">
  <img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg?style=for-the-badge" alt="License">
  <img src="https://img.shields.io/badge/Built_with-Rust-e43716?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Platform-Win_|_Mac_|_Linux-lightgrey?style=for-the-badge" alt="Platform">
</p>

<p align="center">
  <a href="https://cluaiz.com/docs"><b>Documentation</b></a> &nbsp;|&nbsp;
  <a href="https://cluaiz.com"><b>Website</b></a> &nbsp;|&nbsp;
  <a href="https://discord.gg/mab8kBURz"><b>Discord</b></a> &nbsp;|&nbsp;
  <a href="https://reddit.com/u/cluaiz"><b>Reddit</b></a> &nbsp;|&nbsp;
  <a href="https://linkedin.com/company/cluaiz"><b>LinkedIn</b></a>
</p>

## What Cluaiz is

**One engine. Every kind of intelligence.**

Models that think, see, listen, speak and remember, behind one OpenAI-compatible API, on your machine
or in the cloud. Files, pages and pictures become text and vectors you can search. Before every load the engine
measures the machine and places the model: layers on the GPU where speed matters, the rest in RAM, and MoE experts
streamed from the disk when memory runs out. Windows, macOS or Linux, with a GPU or without one.

Agents read, edit and run commands, and ask before they touch. Skills, plugins and MCP servers teach it something
new, and flows carry the steps so you do not have to. Memory keeps only what you confirm, every action stays inside
the limits you set, and an audit log records all of it. It fits your machine, works where you are, and grows with
what you build.

## Why Cluaiz

- **Full speed, without the tuning.** On the same model file and placement, generation is equal to official
  llama.cpp on the GPU, on a GPU + RAM split and on the CPU, with one exception listed in the
  [benchmarks](test/benchmarks/README.md). The planner chooses the GPU layers, context and KV cache for you;
  `cluaiz show` prints why, and every value can still be set by hand.
- **It measures before it loads.** Free GPU memory and RAM are read on every load, so the plan matches the machine
  as it is right now: on an L4, 14 of 16 models loaded within 0.09 GB of the planned VRAM
  ([run 21](test/benchmarks/planner-and-vram.md#run-21)). A model that cannot fit is refused with the
  amount missing, and a load that fails steps back to fewer GPU layers instead of crashing.
- **A MoE model larger than VRAM + RAM, streamed from the SSD.** Qwen3.6 35B A3B (18.2 GB file) on an L4 with
  8 GB VRAM free and a 12 GB RAM limit: Cluaiz generated 19.8-25.1 tokens/s on chat and long prompts with its MoE
  cache and expert prefetch, against 9.1-11.0 for llama.cpp at its defaults (about 2.2x) and 15.9-20.7 for llama.cpp
  with its own MoE cache ([SSD streaming benchmark](test/benchmarks/ssd-streaming-moe.md)).
- **One engine for chat, voice, vision and embeddings.** Chat, vision, speech, transcription and embeddings share one process, one
  model store and one API, so an app does not wire up a separate server for each.
- **Works with what you already use.** Any OpenAI client can point at Cluaiz, and MCP clients such as Claude
  Desktop and Cursor reach it through `cluaiz mcp-server`, and MCP servers that run over HTTP are added with
  `cluaiz mcp add <name> --url <url>`.
- **You stay in control.** Agents act in a sandboxed mode by default, ask before changes outside the workspace,
  and every write, command and admin call goes to an audit log. WebAssembly plugins run with no file access and
  only the network they declare.

## What you can do with it

| | What you get | Read more |
| :--- | :--- | :--- |
| **Chat** | Text chat with tool calls and thinking models, streaming or not | [Chat](docs/api/inference/chat_completions.md) |
| **Vision and audio in chat** | Images, audio, video frames and documents in a message, with a model that has a vision or audio projector | [Chat attachments](docs/api/inference/chat_completions.md) |
| **Speech and transcription** | Text-to-speech (Kokoro and other ONNX families) and speech-to-text (Whisper, Moonshine) | [Speech](docs/api/inference/audio_speech.md), [Transcriptions](docs/api/inference/audio_transcriptions.md) |
| **Embeddings and documents** | Text and image vectors; PDF, DOCX, text and code turned into text, chunks or vectors, with or without a model | [Embeddings](docs/api/inference/embeddings.md), [Ingest](docs/api/inference/ingest.md) |
| **Big models on small machines** | A planner measures free GPU memory and RAM on every load and picks the placement, context size and KV cache type; MoE experts can sit in RAM | [Placement](docs/engine/llm_optimization.md), [MoE](docs/engine/moe_ssd_streaming.md) |
| **Cloud models the same way** | Any OpenAI-compatible provider used as `<provider>/<model>`, with the Engine's memory and tools | [Providers](docs/engine/providers.md) |
| **Your existing apps** | OpenAI-compatible routes, plus CEP (JSON-RPC 2.0) over HTTP, MCP or a C library for everything engine-specific | [API](docs/api/README.md), [CEP](docs/api/cep/README.md) |
| **A coding agent** | `cluaiz agent` reads, searches and edits files and runs commands in a folder, asking before each change | [Agent](docs/cmd/agent.md) |
| **Tools** | Skills, plugins and MCP servers from the [cluaiz-tools](https://github.com/cluaiz/cluaiz-tools) registry, with permission levels, approvals and an audit log | [Tools](docs/engine/tools.md) |
| **Memory and projects** | Project rules, notes kept between conversations that become memory only when you confirm them, and a view of what reached the model | [Chat memory](docs/api/inference/chat_completions.md) |
| **Workflows** | Reusable flows the engine runs (schedules and triggers are in progress) | [Automation](docs/automation/README.md) |

Every feature with its status and the test run that checked it: [features](docs/reference/features.md).

## Works with

- **[Cluaiz App](https://github.com/cluaiz/cluaiz-app):** the desktop app. It talks to a running `cluaiz serve`, so
  a model is loaded once.
- **[Cluaiz Tools](https://github.com/cluaiz/cluaiz-tools):** the registry of skills, plugins and MCP servers that
  the Engine installs. It takes pull requests.

## Quick start

**1. Install.** The script downloads the release for your system and the GPU pack for your GPU, and checks every
file against the release's checksums.

```powershell
# Windows
powershell -c "irm https://cluaiz.com/install.ps1 | iex"
```

```bash
# Linux and macOS
curl -fsSL https://cluaiz.com/install.sh | bash
```

Windows 10 / 11 x64; Linux x64 or ARM64 with glibc 2.39 or newer (Ubuntu 24.04, Debian 13, Fedora 40 or newer);
macOS 14 or newer on Apple Silicon. Intel Macs and Windows on ARM are not supported yet.
Update with `cluaiz self update`; remove with `cluaiz self uninstall`.

**2. Get a model and chat.** A name from the [cluaiz.com](https://cluaiz.com) catalogue or any Hugging Face
repository; it is downloaded on first use.

```bash
cluaiz run qwen3.8:27b "Explain a KV cache in two sentences."
```

Other models from the catalogue work the same way, for example `gemma-4:26b`.

**3. Let the agent work in a folder:** `cluaiz agent`, from inside the folder.

![cluaiz agent with Qwen3.5-4B writes hello.py, runs it and reports the changed file (2x speed)](docs/assets/engine/agent.gif)

**4. Serve it to your apps** and call the OpenAI-compatible API.

```bash
cluaiz serve
```

```bash
curl http://127.0.0.1:8000/v1/chat/completions \
  -H "Authorization: Bearer $(cat ~/.cluaiz/session.token)" \
  -H "Content-Type: application/json" \
  -d '{"model": "<model id from cluaiz list>", "messages": [{"role": "user", "content": "Hello"}]}'
```

`cluaiz list` shows the installed model IDs, `cluaiz token show` the API token, and `cluaiz --help` every command.
The full walk-through: [quick start](docs/get-started/quickstart.md).

## Speed

Under the hood, GGUF models run on [llama.cpp](https://github.com/ggml-org/llama.cpp) and speech and vision models on
ONNX Runtime, inside an engine written in Rust ([architecture](docs/get-started/architecture.md)).

Same model file, same placement, same settings, Cluaiz against official llama.cpp's `llama-bench`:

| | Result |
| :--- | :--- |
| Generation (GPU, GPU + RAM split, CPU) | Equal, within the run-to-run noise; one exception: a 2-bit model on a 1 vCPU server, 13-20% slower |
| Prompt, fully on the GPU | Equal on an RTX 3050 laptop and a Tesla T4; llama.cpp up to 11% faster on an L4 |
| Prompt, split across GPU and RAM | Up to 1.6x faster through the chat API on the T4 (even prompt chunks); equal with `cluaiz bench` |
| MoE model larger than VRAM + RAM (Qwen3.6 35B A3B, L4 with 8 GB VRAM free, 12 GB RAM) | Generation 1.7-2.3x faster than llama.cpp at its defaults, and 1.2-1.4x faster than llama.cpp with its own MoE cache ([SSD streaming benchmark](test/benchmarks/ssd-streaming-moe.md)) |

Every table, machine and run: [benchmarks](test/benchmarks/README.md).

## Supported hardware

| Hardware | Backend | Tested on |
| :--- | :--- | :--- |
| **NVIDIA GPU**, Windows and Linux | CUDA 12 / CUDA 13 pack | RTX 3050 Laptop 4 GB (Windows 11), Tesla T4 16 GB (Ubuntu 24.04), L4 23 GB (Debian 13) |
| **Apple Silicon**, macOS 14+ | Metal | MacBook Air M5 16 GB ([run](test/benchmarks/release-and-platforms.md#mac-m5)), virtual M1 on macOS 15 |
| **AMD Radeon, Intel Arc**, Windows and Linux | Vulkan pack | Built, not yet run on this hardware |
| **x86-64 CPU** (SSE4.2 to AVX-512) | CPU, best build picked at start | Ryzen 7 7435HS, Xeon with AVX-512 / AMX, EPYC VPS with 1 vCPU and 3.8 GB RAM ([run](test/benchmarks/release-and-platforms.md#run-19)) |
| **ARM64 Linux** | CPU (NEON and newer) | AWS Graviton2, Ubuntu 24.04 |

ROCm/HIP, OpenVINO, SYCL and CANN builds are not part of the release yet. Packages and every run:
[platforms](docs/deployments/platforms.md).

## Documentation

| I want to | Start here |
| :--- | :--- |
| Install and run a first model | [Quick start](docs/get-started/quickstart.md) |
| Use the terminal chat or the agent | [Chat](docs/cmd/chat.md), [Agent](docs/cmd/agent.md), [all commands](docs/cmd/commands_reference.md) |
| Call it from my app | [HTTP API](docs/api/README.md), [CEP](docs/api/cep/README.md) |
| Know which models work | [Model families](RELEASE.md) |
| See how it works inside | [Architecture](docs/get-started/architecture.md), [engine crates](engine/README.md) |
| Fix a problem | [Troubleshooting](docs/get-started/troubleshooting.md) |
| Find any page | [Docs index](docs/index.md) |

## Community

Questions, bugs and ideas: [issues](https://github.com/cluaiz/cluaiz/issues) or [Discord](https://discord.gg/mab8kBURz).

## License

Cluaiz is released under the **Apache License 2.0**. See [LICENSE](LICENSE).
