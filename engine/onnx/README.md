# 🎙️ Cluaiz ONNX Audio & Multimodal Engine (`engine/onnx`)

Unified ONNX compute engine for Audio STT (Whisper, Moonshine), TTS (Kokoro, CosyVoice, Matcha, Chatterbox, Vits), and Multimodal Vision.

## 📐 Architecture

```mermaid
graph TD
    A["OnnxEngine"] --> B["audio/stt: Whisper & Moonshine STT"]
    A --> C["audio/tts: Voice Synthesis"]
    A --> D["vision: Image Encoding"]
    A --> E["text: Text Embeddings"]
    A --> F["cudnn_driver: cuDNN 9 Runtime Provisioning"]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`audio/stt/`](src/audio/stt/) | Typed PCM and legacy source entries for supported uncached Whisper (`input_features`) and Moonshine (`input_values`) encoder/decoder graphs. |
| [`audio/stt/families/`](src/audio/stt/families/) | Modular STT family adapters (`whisper.rs` for fixed-window Mel-spectrogram encoders, `moonshine.rs` for variable-length raw-waveform encoders). |
| [`audio/tts/`](src/audio/tts/) | Existing family pipelines for voice synthesis; family acceptance is measured separately. |
| [`cudnn_driver.rs`](src/cudnn_driver.rs) | Detects, downloads, SHA-256 verifies, and extracts pinned NVIDIA cuDNN 9 (`9.8.0.87`) runtime shared libraries into `<cluaiz_home>/engine/drivers/`. |
| [`vision.rs`](src/vision.rs) | CLIP and ViT visual embedding extraction. |
| [`engine.rs`](src/engine.rs) | ONNX Runtime session pooling and execution. |
| [`media.rs`](src/media.rs) | Typed audio tasks/results, session probing, captured provider policy and processor/signature validation. |

Image byte decoding now calls `cluaiz_media::image::decode_image` directly with core media limits. ONNX retains its current CLIP resize/normalization step; processor-config-driven normalization and runtime/API vision dispatch are separate pending phases.

STT input preparation directly imports shared source/audio/resample functions. Original encoded audio is decoded to interleaved F32 once, then ONNX explicitly chooses mono, peak limiting and its configured model sample rate. The old stt/loader.rs and its exported entry are removed; downstream Rust callers must use the actual shared APIs. Task-specific STT/TTS/vision acceptance remains separate from provider and compilation checks.

## Provider selection reference

- **Owner:** `engine.rs` owns primary, encoder and vision session construction. These loaders attempt CUDA, then Windows DirectML, then CPU. `n_gpu_layers: 0` bypasses their GPU attempts.
- **cuDNN 9 runtime provisioning:** Before registering the CUDA Execution Provider on an NVIDIA system, `cudnn_driver::ensure_cudnn_ready_for_cuda` checks whether `cudnn64_9.dll` (Windows) or `libcudnn.so.9` (Linux) is present in `<cluaiz_home>/engine/drivers/` or the system library search path. If missing, it attempts a single download per process of the official NVIDIA redistributable pinned in `src/data/cudnn_sources.json`, verifies its byte length and SHA-256 digest, and extracts the runtime libraries into `engine/drivers/`. Can be disabled via `CLUAIZ_DISABLE_CUDNN_AUTODOWNLOAD=1`.
- **Registration failure:** CUDA and DirectML attempts use `error_on_failure()` so a failed registration cannot be reported as successful GPU registration. A registered provider still does not prove every operator executes on the GPU; unsupported operators can use CPU.
- **TTS execution failure:** `permits_cpu_execution_retry` identifies the observed Windows missing-cuDNN execution error and DirectML `80070057` operator failure. The runtime may retry a whole, undelivered TTS request once on CPU. Tensor shape/type errors and missing package assets do not qualify; a CPU request cannot loop back into this retry.
- **DirectML configuration:** One private `try_directml_session` implementation disables memory patterns and parallel execution, as required by the [ONNX Runtime DirectML contract](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html#configuration-options). Non-Windows builds skip this attempt and continue to the CPU path. No forwarding-only platform module is used.
- **Auxiliary sessions:** `media.rs` owns `build_session()` with captured settings, strict CUDA registration, Windows DirectML fallback and CPU fallback. Main legacy loaders retain their own established construction flow. Some TTS families still construct sessions directly; those paths are not comprehensively consolidated or certified.
- **Resource fallback:** The existing last CPU retry disables the CPU arena and lowers graph optimization. It does not implement an explicit SSD streaming allocator; existing source log labels must not be treated as evidence of SSD execution.
- **Verification boundary:** Small real ONNX graph tests check CPU execution and retry-builder preservation after provider failure. They do not establish speech quality, visual embeddings, GPU execution, Linux builds, memory peaks or API task routing.

## Typed audio contract

- **Input/output:** `AudioInput::Text` -> `AudioOutput::WavDataUri`; shared `DecodedAudio` -> `AudioOutput::Transcript`. `execute_prepared_audio` rejects a task/modality mismatch. The TTS family computation and WAV encoder are reused through `route_tts_text`; no new synthesis algorithm is introduced.
- **STT graph selection:** `load_audio_package` probes actual sessions from the selected package for either `input_features` (Mel-spectrogram encoder, e.g. Whisper) or `input_values` (raw-waveform encoder, e.g. Moonshine) paired with an uncached decoder declaring `input_ids`, `encoder_hidden_states` and `logits`. Dtypes, ranks, static dimensions and encoder/decoder hidden-state compatibility are checked. Ambiguous/missing graphs or cache-only packages return errors. No fixed empty KV tensor is injected.
- **Processor:** For `input_features` encoders, `preprocessor_config.json` must supply positive feature size, sampling rate, hop, FFT, sample count and frame count. For `input_values` encoders, `preprocessor_config.json` supplies `sampling_rate` (and optional `do_normalize` / `n_samples`), while `config.json` / `tokenizer.json` supply `decoder_start_token_id` (`bos_token_id`), `eos_token_id`, and `max_position_embeddings`. Unsupported lengths fail instead of silently discarding recording tails.
- **Failures:** encoder/decoder run errors, invalid tensor extraction, missing tokenizer/text and stopped output consumer propagate. Existing spectrogram, language probing and greedy sampling remain model-owned routines; universal language/quality equivalence is not asserted.
- **Audio8:** its former single-shot guessed inputs and fabricated codec expansion do not implement the exported stateful AR contract and are removed. Synthesis explicitly returns not implemented. The installed package also lacks tokenizer/reference-voice assets; adding those alone does not supply the missing native runtime.

## Embedding execution contract

- **Owners:** `vision.rs` prepares shared RGB using the package processor; `text.rs` supplies real tokenizer/graph inputs, validates output shape and applies explicit pooling plus L2 normalization. Both image and text use `execute_embedding`; the existing `EmbeddingDriver` image method delegates to it.
- **Sessions:** `load_embedding_package` builds only the selected primary graph through the same captured provider policy as auxiliary audio sessions. `load_vision_model` uses this real owner and replaces the pool rather than appending stale models.
- **Supported processor:** batch-one F32 NCHW/NHWC with one unambiguous RGB axis; config-controlled direct/shortest-edge resize, supported resampling, center crop without padding, rescale and per-channel normalize. Required metadata is not inferred from a model's folder name. Explicit legacy CLIP feature-extractor scaling follows the [pinned integer normalization implementation](https://github.com/huggingface/transformers/blob/v4.11.3/src/transformers/image_utils.py); interpolation parity with Pillow is unverified.
- **Supported output:** one pooled rank-two vector, or text rank-three sequence with package-declared CLS or attention-mask mean pooling. Ambiguous outputs, unsupported inputs/dtypes and invalid vector norms return errors. No empty synthetic image/text tensors satisfy another graph branch.
- **Integration:** the runtime invokes computation on its blocking worker; API `/v1/embeddings` translates text/image requests and reuses existing file authorization. This produces retrieval embeddings, not image Q&A/OCR prose.
- **Scope:** all-family, all-language, quality-ranking, GPU/Linux/macOS, peak allocation and simultaneous chat/audio planning are not certified by these contracts. Installed packages missing processor/tokenizer/pooling assets need those genuine assets before their route can succeed.
