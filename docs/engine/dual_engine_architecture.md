# Two Runtimes: llama.cpp and ONNX Runtime

The Engine runs models through two runtimes, inside one process.

| Runtime | Model files | Used for | Code |
|---|---|---|---|
| llama.cpp | GGUF | Chat (text, images, audio input), Qwen3-TTS speech, GGUF embeddings (bge-m3, EmbeddingGemma) | `engine/llama/`, hosted by `engine/runtime/src/host.rs`, `speech_host.rs`, `embedding_host.rs` |
| ONNX Runtime | ONNX | TTS (Kokoro, Piper), STT (Whisper), text and image embeddings | `engine/onnx/`, hosted by `engine/runtime/src/onnx_host.rs` |

llama.cpp is built into the program. Its GPU backends (CUDA 12, CUDA 13, Vulkan, Metal on macOS)
are libraries next to the executable. ONNX Runtime runs on the CPU, or on CUDA when the ONNX GPU
pack is installed.

## Sharing memory

The two runtimes do not share an allocator, so the Engine shares memory between them by rules:

- The planner (`engine/plan`) places the chat model with a measured safety margin
  (`vram_margins.json`, `engine/runtime/src/vram_use.rs`).
- When memory is short for a load or a request, idle models of the other kinds (ONNX, GGUF
  speech, GGUF embedding) are unloaded first (`engine/runtime/src/media_room.rs`). A model in use
  is never unloaded, and the chat model is never on that list.
- A model that fails to load on the GPU is loaded on the CPU the next time.

## Images in chat

A vision chat model reads images itself through llama.cpp's multimodal projector (`mmproj`). The
ONNX image embedding models (for example CLIP) only return vectors through `/v1/embeddings`; they
are not used to answer chat messages.

See also: [ARCHITECTURE.md](../../ARCHITECTURE.md), [llm_optimization.md](llm_optimization.md).
