# 🏛️ Direct Architecture Tree: Cluaiz (Old vs New)

> **Document Type:** Comprehensive Direct Folder & File Mapping Tree (NO `crates/` intermediate folder)  
> **Reference File:** [`old-folder-tree`](old-folder-tree)  
> **Updated:** Developer Hub Removed, Direct `engine/` Folder Structure, Complete ONNX Audio/Vision Integration  
> **Status:** APPROVED PRODUCTION BLUEPRINT  

---

## 📌 Legend & Status Tags

* **`[UNCHANGED]`** : File aur folder 100% wahi hai, koi badlaav nahi.
* **`[DIRECT ENGINE]`** : Direct `engine/` folder ke andar clean structure (No `crates/` intermediate folder).
* **`[100% PRESERVED]`** : Aapka feature (Model Downloader, Registry, Audio STT/TTS, Llama Fork) 100% intact hai.
* **`[MERGED - SSOT]`** : Duplicate parsers ya samplers ko merge karke single source of truth banaya gaya hai.
* **`[DELETED - DEAD CODE]`** : Jo code `if false` par band tha ya dead kachra tha (e.g. `expert_offloading`, `asm_kernels.rs`).
* **`[DELETED - REPLACED]`** : `developer_hub/` ko permanently delete kiya gaya kyunki ab `cluaiz-app` exist karta hai.
* **`[NEW README]`** : Har folder aur sub-folder me architectural `README.md` document.

---

## 🌳 The Direct Architecture Tree (Direct Folders, No `crates/`)

```text
Cluaiz-Technologies/cluaiz/
├── 📝 README.md                                          [NEW README - Cluaiz Architecture Overview & Mermaid Diagram]
├── ⚙️ Cargo.toml                                         [Workspace Manifest: members = ["engine/*", "api", "cmd", "tools/*", "tests"]]
│
├── 📁 .cargo/                                             [UNCHANGED]
│   └── ⚙️ config.toml                                    [UNCHANGED]
│
├── 📁 .github/                                            [UNCHANGED]
│   ├── 📝 README.md                                      [UNCHANGED]
│   ├── 📁 scripts/                                       [UNCHANGED]
│   │   ├── 🐍 registry_aggregator.py                     [UNCHANGED]
│   │   └── 🐍 secure_registry_builder.py                 [UNCHANGED]
│   ├── 📁 workflows/                                     [UNCHANGED]
│   │   ├── 📝 README.md                                  [UNCHANGED]
│   │   ├── ⚙️ cluaize-cmd.yml                            [UNCHANGED]
│   │   ├── ⚙️ cluaize-engine.yml                         [UNCHANGED]
│   │   ├── ⚙️ cluaize-kernel-llama.yml                   [UNCHANGED]
│   │   ├── ⚙️ cluaize-kernel-onnx.yml                    [UNCHANGED]
│   │   ├── ⚙️ publish-registry.yml                       [UNCHANGED]
│   │   └── ⚙️ release-drafter.yml                        [UNCHANGED]
│   └── ⚙️ FUNDING.yml                                    [UNCHANGED]
│
├── 📁 assets/                                             [UNCHANGED - 100% PRESERVED]
│   ├── 📝 README.md                                      [NEW README]
│   ├── 📁 ipa_dictionary/                                [UNCHANGED - All 160+ Phonetic Dictionaries Intact]
│   │   ├── 📄 -ga.txt ... 📄 zh.txt                      [UNCHANGED - 100% Intact]
│   ├── 🖼️ Banner.png                                     [UNCHANGED]
│   ├── 🖼️ CLUAIZ AI kernel architecture overview.png     [UNCHANGED]
│   ├── 🖼️ Futuristic AI system architecture dashboard.png[UNCHANGED]
│   ├── 🖼️ hf-download-model.png                          [UNCHANGED]
│   ├── 📄 logo.ico                                       [UNCHANGED]
│   ├── 🖼️ logo.png                                       [UNCHANGED]
│   └── 🖼️ website-download-model.png                     [UNCHANGED]
│
├── ❌ developer_hub/                                      [DELETED - PERMANENTLY REMOVED: Replaced by cluaiz-app Desktop]
│
├── 📁 docs/                                               [UNCHANGED - 100% PRESERVED]
│   ├── 📝 README.md                                      [UNCHANGED]
│   ├── 📁 api/                                           [UNCHANGED]
│   │   ├── 📝 README.md                                  [UNCHANGED]
│   │   └── 📁 system/                                    [UNCHANGED]
│   │       ├── 📝 compare.md                             [UNCHANGED]
│   │       ├── 📝 info_hardware.md                       [UNCHANGED]
│   │       ├── 📝 live.md                                [UNCHANGED]
│   │       └── 📝 plan.md                                [UNCHANGED]
│   ├── 📁 cel/                                           [UNCHANGED]
│   │   └── 📝 cel-syntax-reference.md                    [UNCHANGED]
│   └── 📝 architecture.md                                [NEW README - Blueprint Reference]
│
├── 📁 tools/                                              [UNCHANGED - 100% PRESERVED]
│   ├── 📝 README.md                                      [NEW README]
│   └── 📁 cluaiz-builder/                                [UNCHANGED]
│       ├── 📝 README.md                                  [NEW README]
│       ├── ⚙️ Cargo.toml                                 [UNCHANGED]
│       └── 📁 src/                                       [UNCHANGED]
│           ├── 🦀 local_ci.rs                            [UNCHANGED]
│           ├── 🦀 main.rs                                [UNCHANGED]
│           └── 🦀 packager.rs                            [UNCHANGED]
│
├── 📁 scripts/                                            [UNCHANGED - 100% PRESERVED]
│   └── 🐚 build_all.ps1 / setup scripts                  [UNCHANGED]
│
├── 📁 tests/                                              [INTEGRATION TEST SUITE]
│   ├── 📝 README.md                                      [NEW README]
│   ├── ⚙️ Cargo.toml                                     [UNCHANGED]
│   ├── 📁 fixtures/                                      [Real GGUF Headers Only, KBs]
│   │   └── 📄 gemma-26b.header.gguf                      [NEW FIXTURE]
│   └── 📁 tests/                                         [PRESERVED & EXPANDED]
│       ├── 🦀 test_real_system_info.rs                   [UNCHANGED - 4/4 Tests Pass]
│       ├── 🦀 test_live_system_plan_api.rs               [UNCHANGED]
│       ├── 🐚 test_plan_api.ps1                          [UNCHANGED]
│       └── 🦀 plan_rules_test.rs                         [Candidate A-F Assertion Tests]
│
│
═══════════════════════════════════════════════════════════════════════════════
                 🚀 DIRECT ENGINE LAYER (`engine/` - NO CRATES FOLDER)
═══════════════════════════════════════════════════════════════════════════════
│
├── 📁 engine/                                             [DIRECT ENGINE ROOT - ALL COMPUTE SUBSYSTEMS]
│   ├── 📝 README.md                                      [NEW README - Compute Engine Pipeline Architecture]
│   │
│   │──────────────────────────────────────────────────────────────────────────
│   │ 1. engine/core (Shared Foundation, Settings Snapshots & Environment)
│   │    Purana Path: inference-engine/engines/core/src/environment/ & schema/
│   │──────────────────────────────────────────────────────────────────────────
│   ├── 📁 core/
│   │   ├── 📝 README.md                                  [NEW README]
│   │   ├── ⚙️ Cargo.toml
│   │   └── 📁 src/
│   │       ├── 🦀 lib.rs
│   │       ├── 🦀 env.rs                                 [Path Resolving & Config Dirs - 100% PRESERVED]
│   │       ├── 🦀 units.rs                               [GIB = 1024*1024*1024 Constant]
│   │       ├── 🦀 error.rs                               [Engine Shared Errors]
│   │       ├── 🦀 contracts.rs                           [Engine Traits & Contracts]
│   │       └── 📁 settings/                              [In-Memory Cached Settings Snapshots]
│   │           ├── 🦀 mod.rs
│   │           ├── 🦀 gguf.rs                            [gguf_metadata_headers.json Schema]
│   │           └── 🦀 optimization.rs                    [llm_optimization.json Schema]
│   │
│   │──────────────────────────────────────────────────────────────────────────
│   │ 2. engine/hardware (Silicon Truth: Hardware Profiler & RAM Governor)
│   │    Purana Path: inference-engine/engines/core/src/hardware/system_info.rs
│   │──────────────────────────────────────────────────────────────────────────
│   ├── 📁 hardware/
│   │   ├── 📝 README.md                                  [NEW README]
│   │   ├── ⚙️ Cargo.toml
│   │   └── 📁 src/
│   │       ├── 🦀 lib.rs
│   │       ├── 🦀 static_info.rs                         [CPU & RAM Probing - 100% PRESERVED]
│   │       ├── 🦀 live.rs                                [Live Free RAM / VRAM Monitoring]
│   │       ├── 🦀 calibrate.rs                           [Calibrated 128MB Memory Benchmark & 950 GFLOPS CPU Boost]
│   │       ├── 🦀 cache.rs                               [Cached NVML Session]
│   │       └── 📁 gpu/
│   │           ├── 🦀 mod.rs
│   │           ├── 🦀 nvml.rs                            [Single NVML Initializer]
│   │           └── 🦀 backend.rs                         [Direct GGML Device Query]
│   │
│   │──────────────────────────────────────────────────────────────────────────
│   │ 3. engine/models (Downloader, Vault, Registry, Taxonomy & GGUF Reader)
│   │    Purana Path: engines/src/models/{fetcher, registry, taxonomy, prober}
│   │    ⭐ AAPKA MODEL DOWNLOADER & REGISTRY YAHAN 100% SURVIVE KAREGA ⭐
│   │──────────────────────────────────────────────────────────────────────────
│   ├── 📁 models/
│   │   ├── 📝 README.md                                  [NEW README]
│   │   ├── ⚙️ Cargo.toml
│   │   └── 📁 src/
│   │       ├── 🦀 lib.rs
│   │       │
│   │       ├── 📁 fetcher/                               [AAPKA HUGGINGFACE DOWNLOADER - 100% PRESERVED]
│   │       │   ├── 📝 README.md                          [NEW README]
│   │       │   ├── 🦀 mod.rs                             [100% PRESERVED]
│   │       │   ├── 🦀 client.rs                          [Registry Client & Telemetry - 100% PRESERVED]
│   │       │   ├── 🦀 downloader.rs                      [Chunked Downloader & Parallel Streams - 100% PRESERVED]
│   │       │   ├── 🦀 auto_heal.rs                       [Auto-healing Corrupted Weights - 100% PRESERVED]
│   │       │   ├── 🦀 bundler.rs                         [Multi-file Bundle Acquisition - 100% PRESERVED]
│   │       │   └── 🦀 hf_hub.rs                          [HuggingFace Hub REST API - 100% PRESERVED]
│   │       │
│   │       ├── 📁 registry/                              [AAPKA MODEL VAULT & ROSTER - 100% PRESERVED]
│   │       │   ├── 📝 README.md                          [NEW README]
│   │       │   ├── 🦀 mod.rs                             [100% PRESERVED]
│   │       │   ├── 🦀 installed_state.rs                 [Installed Models Registry - 100% PRESERVED]
│   │       │   ├── 🦀 roster.rs                          [Remote & Local Rosters - 100% PRESERVED]
│   │       │   ├── 🦀 vault.rs                           [Category Directories & Storage - 100% PRESERVED]
│   │       │   └── 🦀 auditor.rs                         [Model Hardware Compatibility Auditor - 100% PRESERVED]
│   │       │
│   │       ├── 📁 taxonomy/                              [AAPKA TAXONOMY & CAPABILITIES - 100% PRESERVED]
│   │       │   ├── 📝 README.md                          [NEW README]
│   │       │   ├── 🦀 mod.rs                             [100% PRESERVED]
│   │       │   ├── 🦀 stt.rs                             [Whisper Taxonomy - 100% PRESERVED]
│   │       │   └── 🦀 tts.rs                             [Kokoro / Sherpa Taxonomy - 100% PRESERVED]
│   │       │
│   │       ├── 📁 types/                                 [DATA SCHEMAS - 100% PRESERVED]
│   │       │   ├── 🦀 manifest.rs                        [Model Manifests - 100% PRESERVED]
│   │       │   └── 🦀 entities.rs                        [Model Registry Entries - 100% PRESERVED]
│   │       │
│   │       ├── 📁 prober/                                [PROBER & EXTRACTOR - 100% PRESERVED]
│   │       │   ├── 🦀 mod.rs                             [Model Prober Discovery Engine - 100% PRESERVED]
│   │       │   └── 🦀 capability.rs                      [Multimodal Image / Text Detection]
│   │       │
│   │       └── 📁 gguf/                                  [SINGLE AUTHORITATIVE GGUF PROBER - CONSOLIDATED]
│   │           ├── 📝 README.md                          [NEW README]
│   │           ├── 🦀 mod.rs
│   │           ├── 🦀 reader.rs                          [Fast Binary Header Parser]
│   │           ├── 🦀 geometry.rs                        [Layer Counts, Heads, Dims, Experts]
│   │           └── 🦀 kv.rs                              [REAL SWA Sliding Window Attention Math]
│   │
│   │──────────────────────────────────────────────────────────────────────────
│   │ 4. engine/plan (Typed Placement Planner: Pure Logic, 100% Unit Testable)
│   │    Purana Path: core/hardware/resource_negotiator.rs (2,226 lines God Function)
│   │──────────────────────────────────────────────────────────────────────────
│   ├── 📁 plan/
│   │   ├── 📝 README.md                                  [NEW README]
│   │   ├── ⚙️ Cargo.toml
│   │   └── 📁 src/
│   │       ├── 🦀 lib.rs
│   │       ├── 🦀 types.rs                               [CandidatePlan Enum, HardwareSnapshotInfo, ModelPlanInfo]
│   │       ├── 🦀 budget.rs                              [Safety Buffer Subtraction (RAM & VRAM)]
│   │       ├── 🦀 ctx.rs                                 [Single Context Decision Policy (SSOT)]
│   │       ├── 🦀 cost.rs                                [Transfer Cost & Bus Latency Formulas]
│   │       ├── 🦀 candidates.rs                          [Candidate A (Full GPU), B (GPU KV), D (CPU), F (Hybrid SSD)]
│   │       ├── 🦀 choose.rs                              [Winner Arbitrator: Best Throughput & Stability]
│   │       └── 🦀 report.rs                              [/v1/system/plan JSON Serializer]
│   │
│   │──────────────────────────────────────────────────────────────────────────
│   │ 5. engine/llama (Llama.cpp Submodule Fork + Safe Rust Engine)
│   │    Purana Path: interface-engines/llama/
│   │    ⭐ AAPKA LLAMA.CPP FORK WITH ALL CUSTOM CODE YAHAN 100% SURVIVE KAREGA ⭐
│   │──────────────────────────────────────────────────────────────────────────
│   ├── 📁 llama/
│   │   ├── 📝 README.md                                  [NEW README]
│   │   ├── ⚙️ Cargo.toml
│   │   ├── 🦀 build.rs                                   [CMake & Native CUDA/Vulkan Build Script - 100% PRESERVED]
│   │   │
│   │   ├── 📁 llama.cpp/                                 [AAPKA CUSTOM LLAMA FORK - 100% UNTOUCHED]
│   │   │   ├── 📁 common/
│   │   │   │   ├── 📄 chat.cpp                           [AAPKA CUSTOM CHAT C-ABI - 100% INTACT]
│   │   │   │   ├── 📄 chat.h                             [AAPKA CUSTOM CHAT HEADER - 100% INTACT]
│   │   │   │   └── ...                                   [All llama.cpp common utilities intact]
│   │   │   ├── 📁 src/
│   │   │   │   ├── 📄 llama-kv-cache.cpp                 [AAPKA M-ROPE CONTEXT SHIFT PATCH - 100% INTACT]
│   │   │   │   └── ...                                   [All core llama C++ files intact]
│   │   │   └── 📁 ggml/                                  [GGML Backends (CUDA, Vulkan, CPU) - 100% INTACT]
│   │   │
│   │   ├── 📁 shim/                                      [LIGHTWEIGHT C-SHIM]
│   │   │   ├── 📄 cluaiz_shim.h                          [Dry-run Device Memory & Actual n_ctx C-API]
│   │   │   └── 📄 cluaiz_shim.cpp
│   │   │
│   │   └── 📁 src/                                       [SAFE RUST ENGINE - NO DLL, STATIC rlib]
│   │       ├── 🦀 lib.rs                                 [Engine Trait Implementation]
│   │       ├── 🦀 ffi.rs                                 [Raw FFI Declarations]
│   │       ├── 🦀 load.rs                                [Model Instantiation from Plan - 100% PRESERVED]
│   │       ├── 🦀 stream.rs                              [Dynamic Prefix Matching & Token Stream - 100% PRESERVED]
│   │       ├── 🦀 verify.rs                              [Post-load Sanity Check: actual VRAM & n_ctx]
│   │       ├── 🦀 context.rs                             [KV Cache Save & Restore - 100% PRESERVED]
│   │       ├── 🦀 config.rs                              [Optimization Config - 100% PRESERVED]
│   │       ├── 🦀 sampler.rs                             [Native Sampler Chain]
│   │       └── 🦀 template.rs                            [Native Jinja Chat Template]
│   │
│   │──────────────────────────────────────────────────────────────────────────
│   │ 6. engine/onnx (Voice & Audio STT / TTS Engine + Multimodal Vision)
│   │    Purana Path: interface-engines/onnx/
│   │    ⭐ AAPKA WHISPER & KOKORO AUDIO PIPELINE YAHAN 100% SURVIVE KAREGA ⭐
│   │──────────────────────────────────────────────────────────────────────────
│   ├── 📁 onnx/
│   │   ├── 📝 README.md                                  [NEW README - ONNX Audio & Vision Pipeline Blueprint]
│   │   ├── ⚙️ Cargo.toml
│   │   └── 📁 src/
│   │       ├── 🦀 lib.rs                                 [Public API: STT, TTS & Vision]
│   │       ├── 🦀 engine.rs                              [ONNX Runtime Session Management: DirectML, CUDA, CoreML, CPU]
│   │       ├── 🦀 vision.rs                              [Vision Encoder & Image Preprocessor - 100% PRESERVED]
│   │       │
│   │       └── 📁 audio/                                 [ALL AUDIO PIPELINES - 100% PRESERVED]
│   │           ├── 📝 README.md                          [Audio Architecture: STT Mel-Spectrogram + TTS Vocoder Flow]
│   │           ├── 🦀 mod.rs                             [Audio Subsystem Entry Point]
│   │           ├── 🦀 config.rs                          [Audio Sample Rates & Buffer Settings]
│   │           │
│   │           ├── 📁 stt/                               [WHISPER SPEECH-TO-TEXT - 100% PRESERVED]
│   │           │   ├── 📝 README.md                      [Whisper Decoding & Mel-Bank Specs]
│   │           │   ├── 🦀 mod.rs                         [STT Module Entry]
│   │           │   ├── 🦀 loader.rs                      [Whisper ONNX Model Loader]
│   │           │   ├── 🦀 decoder.rs                     [Autoregressive Audio Token Decoder]
│   │           │   ├── 🦀 mel_bank.rs                    [Mel Filterbank Transformation]
│   │           │   └── 🦀 spectrogram.rs                 [Audio FFT Spectrogram Generator]
│   │           │
│   │           └── 📁 tts/                               [NEURAL TEXT-TO-SPEECH - 100% PRESERVED]
│   │               ├── 📝 README.md                      [TTS Router, G2P & Vocoder Flow]
│   │               ├── 🦀 mod.rs                         [TTS Module Entry]
│   │               ├── 🦀 tts_router.rs                  [Voice Router: Kokoro, CosyVoice, Matcha]
│   │               ├── 🦀 g2p.rs                         [Grapheme-to-Phoneme Converter]
│   │               ├── 🦀 ipa_dictionary.rs              [Phonetic IPA Lookup Engine]
│   │               ├── 🦀 phoneme_map.rs                 [Language Phoneme Mappings]
│   │               ├── 🦀 flow_matching.rs               [Flow Matching Audio Synthesis]
│   │               ├── 🦀 neural_vocoder.rs              [Neural Vocoder: Mel to Raw PCM Waves]
│   │               ├── 🦀 vocoder.rs                     [Audio Post-Processing Filters]
│   │               ├── 🦀 manifest_loader.rs             [Voice Model Manifest Resolver]
│   │               ├── 🦀 family_adapter.rs              [Architecture Family Adapter]
│   │               │
│   │               └── 📁 families/                      [ALL VOICE MODEL FAMILIES - 100% PRESERVED]
│   │                   ├── 🦀 mod.rs
│   │                   ├── 🦀 kokoro.rs                  [Kokoro TTS Engine - 100% PRESERVED]
│   │                   ├── 🦀 cosyvoice.rs               [CosyVoice Pipeline - 100% PRESERVED]
│   │                   ├── 🦀 matcha.rs                  [Matcha-TTS Pipeline - 100% PRESERVED]
│   │                   ├── 🦀 chatterbox.rs              [Chatterbox Adapter - 100% PRESERVED]
│   │                   ├── 🦀 omnivoice.rs               [OmniVoice Adapter - 100% PRESERVED]
│   │                   ├── 🦀 supertone.rs               [Supertone Adapter - 100% PRESERVED]
│   │                   ├── 🦀 audio8.rs                  [Audio8 Adapter - 100% PRESERVED]
│   │                   ├── 🦀 vits_pipeline.rs           [VITS Speech Synthesis - 100% PRESERVED]
│   │                   └── 🦀 logger.rs                  [Audio Synthesis Telemetry]
│   │
│   │──────────────────────────────────────────────────────────────────────────
│   │ 7. engine/runtime (Single Model Host & Multi-Slot Lifecycle Arbitrator)
│   │    Purana Path: interface-engines/dispatcher/ + engines/api/router.rs
│   │──────────────────────────────────────────────────────────────────────────
│   └── 📁 runtime/
│       ├── 📝 README.md                                  [NEW README]
│       ├── ⚙️ Cargo.toml
│       └── 📁 src/
│           ├── 🦀 lib.rs
│           ├── 🦀 host.rs                                [Single Engine Host: Direct static call, NO Library::new]
│           ├── 🦀 policy.rs                              [Multi-slot Arbitrator: chat_slot, vision_slot, audio_slot]
│           ├── 🦀 session.rs                             [Active Stream Semaphore & Cancellation - 100% PRESERVED]
│           └── 🦀 telemetry.rs                           [Live Context & Token Rate Exporter to UI]
│
│
═══════════════════════════════════════════════════════════════════════════════
                 🌐 EXTERNAL API, CLI, AGENTS & SECURITY
═══════════════════════════════════════════════════════════════════════════════
│
├── 📁 api/                                                [DIRECT API SERVER - AXUM HTTP / SSE]
│   ├── 📝 README.md                                      [NEW README - API Gateway Specs]
│   ├── ⚙️ Cargo.toml
│   └── 📁 src/
│       ├── 🦀 lib.rs
│       ├── 🦀 main.rs                                    [Server Binary Entry Point]
│       ├── 🦀 routes.rs                                  [Axum Route Table - 100% PRESERVED]
│       ├── 🦀 state.rs                                   [Application State Container - 100% PRESERVED]
│       ├── 📁 handlers/                                  [ALL API HANDLERS 100% PRESERVED]
│       │   ├── 📁 chat/                                  [100% PRESERVED]
│       │   │   ├── 🦀 mod.rs                             [100% PRESERVED]
│       │   │   ├── 🦀 preflight.rs                       [Preflight Guardrails - 100% PRESERVED]
│       │   │   ├── 🦀 agent_loop.rs                      [Agentic Loop - 100% PRESERVED]
│       │   │   ├── 🦀 non_streaming.rs                   [Non-streaming Chat - 100% PRESERVED]
│       │   │   ├── 🦀 types.rs                           [Chat Schemas - 100% PRESERVED]
│       │   │   └── 🦀 control.rs                         [Stream Control & Cancellation - 100% PRESERVED]
│       │   ├── 🦀 system.rs                              [System Plan & Hardware APIs - 100% PRESERVED]
│       │   ├── 🦀 models.rs                              [Model Management Endpoints - 100% PRESERVED]
│       │   ├── 🦀 audio.rs                               [Audio STT/TTS Endpoints - 100% PRESERVED]
│       │   ├── 🦀 embeddings.rs                          [Embeddings Endpoints - 100% PRESERVED]
│       │   ├── 🦀 fs.rs                                  [Filesystem Endpoints - 100% PRESERVED]
│       │   └── 🦀 ps.rs                                  [Process List Endpoints - 100% PRESERVED]
│       └── 📁 utils/
│           └── 🦀 slots.rs                               [Slot Capability Verification - 100% PRESERVED]
│
├── 📁 cmd/                                                [DIRECT CLI & TUI - AAPKA CMD FOLDER 100% PRESERVED]
│   ├── 📝 README.md                                      [NEW README]
│   ├── ⚙️ Cargo.toml
│   └── 📁 src/
│       ├── 🦀 main.rs                                    [CLI Entry Point - 100% PRESERVED]
│       ├── 📁 assets/                                    [Logos, ASCII Branding - 100% PRESERVED]
│       ├── 📁 cli/                                       [ALL CLI COMMANDS - 100% PRESERVED]
│       │   ├── 🦀 benchmark.rs                           [100% PRESERVED]
│       │   ├── 🦀 pull.rs                                [100% PRESERVED]
│       │   ├── 🦀 run.rs                                 [100% PRESERVED]
│       │   ├── 🦀 inspect.rs                             [100% PRESERVED]
│       │   ├── 🦀 list.rs                                [100% PRESERVED]
│       │   ├── 🦀 storage.rs                             [100% PRESERVED]
│       │   └── 🦀 ps.rs                                  [100% PRESERVED]
│       ├── 📁 core/                                      [App Flow & Dashboard - 100% PRESERVED]
│       └── 📁 ui/                                        [TUI Menus, History, Settings - 100% PRESERVED]
│
├── 📁 agents/                                             [AGENTIC TOOLS, MCP CLIENT & PLUGINS]
│   ├── 📝 README.md                                      [NEW README]
│   ├── ⚙️ Cargo.toml
│   └── 📁 src/
│       ├── 🦀 lib.rs
│       ├── 📁 mcp/                                       [MCP Client Protocol - 100% PRESERVED]
│       ├── 📁 plugins/                                   [Plugin Loader - 100% PRESERVED]
│       ├── 📁 skills/                                    [Skills Router - 100% PRESERVED]
│       └── 📁 execution/                                 [Terminal & Script Runners - 100% PRESERVED]
│
└── 📁 security/                                           [PERMISSION SCHEMA, FS JAIL & APPROVAL GATES]
    ├── 📝 README.md                                      [NEW README]
    ├── ⚙️ Cargo.toml
    └── 📁 src/
        ├── 🦀 lib.rs
        ├── 🦀 auth.rs                                    [Bearer Token Verifier - 100% PRESERVED]
        ├── 🦀 jail.rs                                    [Filesystem Path Jail Guard - 100% PRESERVED]
        ├── 🦀 permission.rs                              [Permission Schema Loader - 100% PRESERVED]
        └── 🦀 gate.rs                                    [Interactive User Approval Gate for Dangerous Ops]
```

---

## 🗑️ Summary of Eliminated Dead Code

Neeche ke saare items production me kabhi execute nahi hote the aur memory/startup block karte the:

1. **`developer_hub/`** -> **Permanently Deleted:** Aapka naya modern `cluaiz-app` exist karta hai.
2. **`expert_offloading/` (7 files, ~4,300 lines)** -> **Permanently Deleted:** Hook `if false` par band tha.
3. **`dma_streamer.rs`** -> **Permanently Deleted:** Dead DMA memory harness.
4. **`asm_kernels.rs` (202 lines)** -> **Permanently Deleted:** Function sirf `return -1;` karta tha.
5. **`hybrid.rs` & `lucebox.rs`** -> **Permanently Deleted:** Hypothetical accelerator stubs.
6. **`speculative.rs`** -> **Permanently Deleted:** Draft model chalta nahi tha, ulta CUDA graphs band karta tha.
7. **`interface-engines/utils/`, `ffi/`, `global_runtime/`** -> **Permanently Deleted:** 3 empty folders.
8. **`inference-drivers/`** -> **Permanently Deleted:** Unused unreferenced crate.
9. **`turbo_quant/` (6 files)** -> **Permanently Deleted:** Orphan uncompiled files.

---

### 🎯 Final Clarity:
* Koi `crates/` intermediate folder nahi hai.
* Sab kuch natural aur direct hai: `engine/`, `api/`, `cmd/`, `agents/`, `security/`.
* `engine/onnx/` ke andar aapka poora Whisper STT, Kokoro TTS, CosyVoice, aur Vision system 100% preserved hai.
* `engine/models/fetcher/` ke andar aapka HuggingFace model downloader 100% preserved hai.
* `engine/llama/llama.cpp/` ke andar aapka custom llama fork with chat C-ABI aur M-RoPE 100% preserved hai.
* Har folder aur sub-folder me `README.md` document architecture samjhane ke liye hoga.

---

## 🗂️ Macro View: Folders-Only Collapsed Tree (Single-Glance Structure)

> **Files Closed:** Neeche saari internal files close/hide kar di gayi hain taaki aapko poore system ki 1-page bird's-eye view picture saaf dikhe.

```text
Cluaiz-Technologies/cluaiz/
├── 📁 .cargo/
├── 📁 .github/
│   ├── 📁 scripts/
│   └── 📁 workflows/
├── 📁 assets/
│   └── 📁 ipa_dictionary/
├── 📁 docs/
│   ├── 📁 api/
│   │   └── 📁 system/
│   └── 📁 cel/
├── 📁 tools/
│   └── 📁 cluaiz-builder/
├── 📁 scripts/
├── 📁 tests/
│   └── 📁 fixtures/
│
├── 📁 engine/                                    [DIRECT COMPUTE SUBSYSTEM]
│   ├── 📁 core/                                  [Settings & Environment]
│   │   └── 📁 settings/
│   ├── 📁 hardware/                              [Hardware Truth & NVML]
│   │   └── 📁 gpu/
│   ├── 📁 models/                                [Model Downloader, Vault & Registry]
│   │   ├── 📁 fetcher/                           (HuggingFace Multi-file Downloader)
│   │   ├── 📁 registry/                          (Installed Models & Vault)
│   │   ├── 📁 taxonomy/                          (STT / TTS Taxonomy)
│   │   ├── 📁 types/                             (Manifests & Entities)
│   │   ├── 📁 prober/                            (Format & Vision Detection)
│   │   └── 📁 gguf/                              (Single GGUF Reader with SWA Math)
│   ├── 📁 plan/                                  [Candidate A-F Placement Planner]
│   ├── 📁 llama/                                 [Llama Engine & Custom Fork]
│   │   ├── 📁 llama.cpp/                         (Aapka Custom Llama Fork with chat.cpp & M-RoPE)
│   │   │   ├── 📁 common/
│   │   │   ├── 📁 src/
│   │   │   └── 📁 ggml/
│   │   └── 📁 shim/                              (Lightweight C-Shim)
│   ├── 📁 onnx/                                  [ONNX Audio & Multimodal Vision]
│   │   └── 📁 audio/
│   │       ├── 📁 stt/                           (Whisper Speech-to-Text)
│   │       └── 📁 tts/                           (Neural Text-to-Speech)
│   │           └── 📁 families/                  (Kokoro, CosyVoice, Matcha, Chatterbox, Vits)
│   └── 📁 runtime/                               [Single Engine Host & Multi-Slot Lifecycle]
│
├── 📁 api/                                       [DIRECT AXUM REST & SSE SERVER]
│   ├── 📁 handlers/
│   │   └── 📁 chat/                              (Agentic & Streaming Chat)
│   └── 📁 utils/
│
├── 📁 cmd/                                       [DIRECT CLI & TUI TERMINAL]
│   ├── 📁 assets/                                (Branding & ASCII Logos)
│   ├── 📁 cli/                                   (Benchmark, Pull, Run, Storage Commands)
│   ├── 📁 core/                                  (Dashboard & App State)
│   └── 📁 ui/                                    (TUI Menus, History, Settings)
│
├── 📁 agents/                                    [MCP CLIENT, PLUGINS & SKILLS]
│   ├── 📁 mcp/                                   (Model Context Protocol)
│   ├── 📁 plugins/                               (Plugin Runner)
│   ├── 📁 skills/                                (Skills Router)
│   └── 📁 execution/                             (Terminal & Script Runner)
│
└── 📁 security/                                  [PERMISSION SCHEMA & JAILS]
```

