# Engine Configuration Reference

The Engine keeps its settings in JSON files in the config folder: `~/.cluaiz/config/`, or
`<CLUAIZ_HOME>/config/`. Missing fields take their defaults.

| File | What it holds | Change it with | Reference |
|---|---|---|---|
| `settings.json` | Load and speed settings, samplers, ONNX, media limits, chat memory, context runtime | `cluaiz config get/set/reset`, CEP `config.get` / `config.set` | below |
| `permission.json` | Port, host, API auth and tokens, security mode, default models | `cluaiz permission`, `cluaiz token`, `cluaiz model set-chat/set-vector` | [permission.json](../engine/permission.md) |
| `hardware_profile.json` | The saved hardware probe (written by the Engine) | `cluaiz calibrate` | [hardware profile](../engine/system_control.md) |
| `vram_margins.json` | What models really took on this GPU (written by the Engine) | — | [two runtimes](../engine/dual_engine_architecture.md) |
| `model_registry.json` | Installed models | `cluaiz pull`, `cluaiz rm` | [model downloading](../get-started/model-downloading.md) |
| `tools_registry.json` | Installed skills, plugins and MCP servers | `cluaiz skill/plugin/mcp …` | [commands](../cmd/commands_reference.md) |

## `settings.json` sections

The schema is `ActiveUserSettings` in `engine/core/src/settings.rs`.

| Section | What it controls | Reference |
|---|---|---|
| `optimization` | GPU layers, context size, KV cache type, flash attention, context shifting, batches, threads, thinking budget | [load and speed settings](../engine/llm_optimization.md) |
| `samplers` | Default sampler values (llama.cpp names); a chat request's own values win | [chat completions](../api/inference/chat_completions.md) |
| `onnx` | ONNX Runtime settings for speech, TTS and vision encoders | [audio speech](../api/inference/audio_speech.md) |
| `media` | Size limits for images, audio, video and documents | [ingest](../api/inference/ingest.md) |
| `chat_memory` | Saving each chat's KV cache to disk: `enabled`, `max_disk_gb`, `max_age_days` | [KV cache reuse](../engine/prefix_caching_architecture.md) |
| `context_runtime` | How the prompt is shared between history, recall, notes and project files (for example `project_files`); a request's `context_policy` overrides single fields | [context](../api/cep/ai-inference/context/README.md) |
| `telemetry_enabled`, `hardware_preset`, `storage_cleanup_policy` | Top-level switches | `cluaiz config list` |

Examples:

```bash
cluaiz config set optimization.n_gpu_layers 20
cluaiz config set optimization.custom_context_window 8192
cluaiz config set chat_memory.max_disk_gb 10
cluaiz config reset
```

`auto` as a value clears an `optimization` field back to the planner's choice.
