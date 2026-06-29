# Zero-Latency Unified FFI Bridging & Driver Decoupling

This document explains the low-level FFI (Foreign Function Interface) bridging and driver decoupling mechanics inside the cluaiz Inference Engine.

---

## 1. Zero-Latency C-ABI Bridging

To bypass the overhead of REST/gRPC or IPC channels during raw tensor copy operations, cluaiz communicates with native drivers via a strict C-ABI bridge. 

```
┌────────────────────────────────────────────────────────┐
│                   cluaiz ENGINE (RUST)                │
│  Manages routing, safety bounds, state, and planning   │
└───────────────────────────┬────────────────────────────┘
                            │ Safe C-ABI FFI (Zero-Copy)
                            ▼
┌────────────────────────────────────────────────────────┐
│                  NATIVE ENGINE DRIVERS                 │
│        Dynamic Library: .dll / .so / .dylib            │
│  Executes SIMD / CUDA / Metal computation graphs       │
└────────────────────────────────────────────────────────┘
```

The FFI structures are annotated with `#[repr(C)]` to guarantee binary compatibility across compilers:

```rust
#[repr(C)]
pub enum PayloadType {
    Bincode = 0,
    RawJson = 1,
}

#[repr(C)]
pub struct ExtensionPayload {
    pub payload_type: PayloadType,
    pub data_ptr: *const u8,
    pub data_len: usize,
}
```

---

## 2. Dynamic Silicon Dispatch (Driver Decoupling)

cluaiz decouples compile-time compute dependencies. The engine binary compiles independently of CUDA or Metal libraries. At startup, the engine scans the host system's compute capabilities and dynamically loads the matching driver library:

1. **Audit Phase:** Scans PCI buses, Metal device lists, or Vulkan registries.
2. **Bind Phase:** Uses dynamic loading system calls (`LoadLibraryW` on Windows, `dlopen` on POSIX) to load the driver `.dll` / `.so`.
3. **Execution Phase:** Resolves function pointers (such as `cluaiz_execute_payload`) and maps the memory layout.

If no dedicated GPU accelerator is present, the engine binds the baseline CPU SIMD library (utilizing AVX2, AVX-512, or ARM Neon instructions).
