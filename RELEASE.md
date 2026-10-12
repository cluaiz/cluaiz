# Cluaiz Inference Engine: Model Format & Architecture Release Matrix

This document defines the official support matrix, package contracts, and the dynamic capability resolution architecture for the Cluaiz Inference Engine.

```mermaid
graph TD
    A["Hugging Face Hub / Local Dir Scan"] --> B["Voting Arbitrator (3-Way Vote)"]
    B --> C["Binary Header Prober (GGUF / ONNX)"]
    B --> D["Metadata Config JSON Prober"]
    B --> E["HF API Pipeline Tags"]
    C & D & E --> F["Capability Resolver"]
    F --> G["Structural DNA Generation (structural_dna.json)"]
    G --> H["Model Registry Entry (model_registry.json)"]
```

---

## 📊 Core Task Support Matrix

| Category / Modality | GGUF Support | ONNX Support | Target Engines & Frameworks |
| :--- | :---: | :---: | :--- |
| **Chat (Instruct / Base)** | ✅ Yes | ❌ No (Roadmap) | Llama.cpp (GGUF) |
| **Embedding (Text Vectors)** | ✅ Yes | ✅ Yes | Llama.cpp (GGUF), ONNX Runtime (ONNX) |
| **Vision (Multimodal Chat)** | ❌ No | ✅ Yes | ONNX Runtime |
| **Audio STT (Speech-to-Text)**| ❌ No | ✅ Yes | ONNX Runtime (Whisper-ONNX) |
| **Audio TTS (Text-to-Speech)**| ❌ No | ✅ Yes | ONNX Runtime (Multi-Family Engines) |

> [!TIP]
> **Universal Precision & Quantization Support**:
> Cluaiz dynamically supports and executes all standard weight precisions natively. This includes **GGUF** (Q4_K_M, Q8_0, Q5, etc.) and **ONNX** (FP32, FP16, INT8, INT4, etc.).

> [!NOTE]
> **Supported Vision Task Types**:
> - `multimodal-dialogue` (Visual Question Answering & Vision Chat)
> - `image-to-text` (Image Captioning)
> - `visual-question-answering` (VQA)
> - `text-to-image` (Image Generation / Stable Diffusion)
> - `text-to-video` (Video Generation)
> - `optical-character-recognition` (Visual OCR / Document Parsing from images & scanned documents)
> - `vision-feature-extraction` (CLIP / Image Embedding Vector extraction)
>
> **Document Ingestion & OCR Processing Modes**:
> - **Native Text PDFs**: Processed efficiently on CPU using the `pdf-extract` engine (fast, zero-VRAM footprint).
> - **Scanned Documents & Images**: Routed dynamically to visual ONNX models (e.g. Qwen-VL, Phi-3-Vision) for character recognition (OCR) and layout parsing.

---

## 🧪 Empirical Hardware Benchmarks (Regression Matrix: 2026-10-12)

*Empirical test results measured against upstream `llama.cpp` (`llama-bench`) using the exact same placement, model files, and compilation on **AMD Ryzen 7 7435HS (8C/16T), 24 GB RAM, NVIDIA GeForce RTX 3050 A (4 GB VRAM), Windows 11, CUDA 13.2**:*

### 1. Generation Throughput (tokens/sec)

| Model (Quantization) | Placement | Cluaiz | Upstream llama.cpp | Match / Delta |
| :--- | :--- | :---: | :---: | :---: |
| **Bonsai 4B (Q1_0)** | Full GPU | **111.0 t/s** | 106.3 t/s | +4% |
| | Split (18/36 blocks GPU) | **27.6 t/s** | 28.2 t/s | -2% |
| **Bonsai 8B** | Full GPU | **72.7 t/s** | 71.9 t/s | +1% |
| | Split (18/36 blocks GPU) | **17.7 t/s** | 18.1 t/s | -2% |
| **Llama 3.2 3B (Q4_K_M)** | Full GPU | **51.9 t/s** | 51.8 t/s | parity |
| | Split (14/28 blocks GPU) | **23.4 t/s** | 24.9 t/s | -6% |
| **Qwen3 4B (Q4_K_M)** | Full GPU | **39.3 t/s** | 38.7 t/s | +2% |
| | Split (18/36 blocks GPU) | **15.9 t/s** | 14.9 t/s | +7% |
| **Gemma 4 E4B (Q4_K_M)** | Full GPU | **32.3 t/s** | 32.0 t/s | +1% |
| | Split (21/42 blocks GPU) | **14.5 t/s** | 14.6 t/s | parity |
| **Gemma 4 26B MoE (13.3 GB)** | Hybrid Split (Attn on GPU, Experts in RAM/SSD) | **16.0 - 18.0 t/s** | OOM / ~4.4 t/s | Stable on 4 GB VRAM |

### 2. Prompt Processing Acceleration (~530 tokens prompt)

| Model | Full GPU (Cluaiz vs llama.cpp) | Split GPU + RAM (Cluaiz vs llama.cpp) | Speedup Ratio |
| :--- | :---: | :---: | :---: |
| **Llama 3.2 3B** | 2578 vs 2446 t/s | **1111 vs 810 t/s** | **1.4x faster** |
| **Qwen3 4B** | 1863 vs 1744 t/s | **810 vs 529 t/s** | **1.5x faster** |
| **Gemma 4 E4B** | 1666 vs 1605 t/s | **705 vs 452 t/s** | **1.6x faster** |
| **Bonsai 8B** | 1352 vs 1334 t/s | **769 vs 385 t/s** | **2.0x faster** |
| **Bonsai 4B** | 2270 vs 2129 t/s | **1186 vs 692 t/s** | **1.7x faster** |
| **Bonsai 27B** | 47 vs 30 t/s (shared VRAM) | **230 vs 110 t/s** | **2.1x faster** |

*Prompt Chunking Note*: Cluaiz splits prompts into equal chunks (e.g. 265 + 265 tokens) instead of fixed 512 chunks, avoiding slow sub-32 token CPU fallback execution on hybrid RAM layers.

### 3. Latency & TTFT Optimizations

* **Time To First Token (TTFT)**: **0.09s - 0.10s** (warm prompt with persistent MiniJinja template cache, reduced from 0.31s).
* **Control Token Safety**: 0 leaked control tokens (`<|...|>`) across all 18 regression test runs.
* **Full Benchmark Reference**: Exhaustive metrics and automated testing scripts are located at [`test/engine_regression/RESULTS.md`](test/engine_regression/RESULTS.md).

---

## 🎙️ Supported Audio & TTS Model Families

The Cluaiz Engine implements strict package validation contracts across 8 core TTS architectural families and 1 core STT family:

### 1. Text-to-Speech (TTS) Families

| Family Name | Internal Identifier | Status | Required Assets / Package Contract | Primary Graph Details |
| :--- | :--- | :--- | :--- | :--- |
| **Kokoro** | `kokoro-v1` | `✅` | Primary model (`*kokoro*.onnx`), `config.json`, `tokenizer.json`/`vocab.json`, and non-empty `voices/` directory containing `.bin`/`.json` voice styles. | Acoustic text-to-phoneme + style latent decoder |
| **Supertonic** | `supertonic-v3` | `✅` | 4-Stage Pipeline: `text_encoder.onnx`, `duration_predictor.onnx`, `vector_estimator.onnx`, and `vocoder.onnx` / `hift.onnx`. | Multistage diffusion synthesis pipeline |
| **VITS / Piper** | `piper-vits` | `✅` | Primary model (`model.onnx`), `config.json`, and phoneme map (`tokens.txt`). | End-to-end VITS / Piper architectures |
| **Matcha-TTS** | `matcha-v1` | `⏳` | Acoustic Flow Matching graph (`matcha.onnx`/`fm_decoder.onnx`) + HiFi-GAN or WaveNext neural vocoder sub-graph. | Flow-matching ODE synthesis |
| **CosyVoice** | `cosyvoice` | `⏳` | `speech_llm.onnx` / `flow.onnx` / `hift.onnx` / `campplus.onnx` (or combined graph). | Autoregressive + flow-matching pipeline |
| **Audio8** | `audio8-codec` | `⏳` | 4-bit `slow_ar` generator + FP16 `codec_decoder` neural vocoder. | Autoregressive token generation |
| **Chatterbox** | `chatterbox` | `⏳` | Semantic generator (`chatterbox_generator.onnx`) + audio codec decoder. | Codec-based language modeling |
| **OmniVoice** | `omnivoice` | `⏳` | Primary model (`model.onnx`) + large companion weight file (`model.onnx.data`) + `genai_config.json`. | External weight data graph |

### 2. Speech-to-Text (STT) Families

| Family Name | Internal Identifier | Format | Primary Graph Details |
| :--- | :--- | :---: | :--- |
| **Whisper** | `whisper` | ONNX | Encoder-Decoder architecture for speech transcription and translation. |

---

## 🧬 Dynamic Capability Discovery & The DNA Handshake

To support thousands of arbitrary Hugging Face repositories framework-free, Cluaiz implements a **3-Way Voting Arbitrator** at boot-scan:

1. **Binary Header Probing**: Probes the first few megabytes of the weight binary (`.gguf` or `.onnx`) to extract truth directly from compiler metadata (e.g., context window size, tensor types, quantization tags).
2. **Metadata Config JSON Ingestion**: Fallbacks to reading `config.json` or model manifests to resolve model architectures.
3. **HF API Pipeline Tags**: Maps HuggingFace repository pipeline tags to categories.
 

---

## 🗂️ Local Model Registry (`model_registry.json`)

- **Primary Path**: `~/.cluaiz/engine/config/model_registry.json` (or fallback to `.cluaiz/engine/config/model_registry.json` in the working directory).
- **Functionality**: Serves as the localized state management vault. It maps every installed model with its absolute path, category, format type (GGUF/ONNX), and validated tasks. 

---

## 💻 Command Line Interface (CLI) Commands

### 1. Download & Install Model
To scan the Hugging Face Hub, register, and download a model:
```bash
cluaiz pull <huggingface-org>/<repo-name>
# Example:
cluaiz pull onnx-community/whisper-large-v3-turbo
```

### 2. Run / Pull with All Assets (Bypass Filters)
To download **all files and supplementary assets** in a repository (e.g. including espeak, style files, voice styles) bypassing format limits:
```bash
cluaiz run <model-id> --all
# or using the shorthand:
cluaiz run <model-id> --a
```

---

## 🔌 API Inference Endpoints

The Cluaiz HTTP server exposes the following endpoints (default port: `8000`):

- **`POST /v1/chat/completions`**: OpenAI-compatible text generation.
- **`POST /v1/audio/execute`**: Multimodal audio execution. Processes both Text-to-Speech (TTS) and Speech-to-Text (STT/Transcription).
- **`POST /v1/embeddings`**: Text embedding vector generation.
- **`POST /v1/ingest/file`**: Native document parser (CPU-based text extraction for text PDFs, images, etc.).
- **`POST /v1/execute/{component}/{function}`**: Invokes custom installed WASM components.

