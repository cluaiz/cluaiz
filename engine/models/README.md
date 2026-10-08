# 📦 Cluaiz Models Subsystem (`engine/models`)

Unified Model Subsystem for Cluaiz: HuggingFace model downloading, registry/vault tracking, multi-modal taxonomy, and single GGUF reader.

## 📐 Architecture

```mermaid
graph TD
    A[fetcher] -->|Downloads GGUF/Safetensors| B[Local Disk Cache]
    B --> C[registry]
    C -->|Catalogs Installed Models| D[Vault & State]
    E[gguf] -->|Parses Geometry & SWA Math| F[ModelGeometry]
    G[taxonomy] -->|Identifies Family| H[STT / TTS / LLM]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`fetcher/`](src/fetcher/) | HuggingFace parallel chunk downloader with auto-heal and integrity checking. |
| [`registry/`](src/registry/) | Installed models catalog, local vault, and state management. |
| [`taxonomy/`](src/taxonomy/) | Audio STT/TTS family classification (Kokoro, Whisper, CosyVoice, etc.). |
| [`prober/`](src/prober/) | Vision mmproj projector and format detection. |
| [`gguf.rs`](src/gguf.rs) | Single unified GGUF reader with correct Sliding Window Attention (SWA) math. |
