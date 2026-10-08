# 🦙 Cluaiz Llama Engine (`engine/llama`)

Direct Rust API wrapping custom `llama.cpp` C/C++ memory. Compiled as an `rlib` to eliminate dual-static memory copies between Exe and DLL.

## 📐 Architecture

```mermaid
graph TD
    App[cluaiz-runtime / cluaiz-api] -->|Direct Rust Call (rlib)| Engine[LlamaEngine]
    Engine --> Native[native/: NativeLlama / Stream / Sampler / Templater]
    Native --> FFI[ffi/: llama_cpp raw FFI]
    FFI --> CustomFork[llama.cpp/: Custom Fork with chat.cpp & M-RoPE]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`native/core.rs`](src/native/core.rs) | Safe model instantiation, parameter loading, and memory envelope binding. |
| [`native/stream.rs`](src/native/stream.rs) | Token generation loop, chunked prefill, and reasoning control emission. |
| [`native/sampler.rs`](src/native/sampler.rs) | Llama.cpp native sampler chain (temp, top_p, min_p, penalties). |
| [`native/templater.rs`](src/native/templater.rs) | Direct C-ABI Jinja chat templating from the custom fork. |
| [`ffi/llama_cpp.rs`](src/ffi/llama_cpp.rs) | Raw `extern "C"` bindings to compiled `llama.cpp`. |
| [`llama.cpp/`](llama.cpp/) | The underlying C/C++ inference engine with custom M-RoPE and chat C-ABI patches. |
