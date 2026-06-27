# Component: Backend Dispatcher (`interface-engines/dispatcher`)

## Technical Specification
- **Purpose:** Acts as the central intelligence node that dynamically routes execution streams (GGUF/ONNX) to native backend FFI boundaries (Llama.cpp/ONNX Runtime).
- **Platform Support:** Windows, Linux, macOS
- **Reusability Level:** High (Core Subsystem Router)

## API Contract (Interface)
- **Props/Struct/Trait:** `BackendDispatcher`, `CallbackData`
- **Export Type:** Public Module (`dispatcher-crate`)
- **Dependencies:** `interface-engines/llama`, `interface-engines/onnx`, `cluaiz_shared`

## Architecture & Sub-Components
- **`lib.rs` (The Core Logic):** Implements the `BackendDispatcher` trait. Evaluates the model manifest (quantization, precision) and selects the hardware-optimal execution backend without modifying the outer Rust engine.
- **Two-Step Discovery Interceptor:** Contains the C-FFI `callback` wrapper that natively buffers output tokens to detect `<TRIGGER:extension:X>` or `<TRIGGER:plugin:X>` sequences. Safely aborts the underlying C++ generation loop to allow the Rust API layer to inject Skill schemas.

## Failure & Recovery Logic
- **Potential Failure Point:** The underlying C++ execution loop (Llama.cpp/ONNX) could enter an infinite generation state or crash due to VRAM overflow.
- **Recovery Logic:** Uses a native `cancel_flag` bound to the `CallbackData`. When a CEL `<TRIGGER>` is detected or a memory boundary is hit, `cancel_flag` is set to `true`, forcing the C++ side to yield execution back to Rust cleanly (returning `false` in the FFI callback).
