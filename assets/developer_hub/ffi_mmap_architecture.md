# cluaiz Zero-Copy Architecture: FFI & mmap()

This document explains the architecture of the cluaiz Engine's Inter-Process Communication (IPC), how `ffi_bridge.rs` currently works, and how to scale it for massive Tensor Data using C FFI and Memory Mapping (`mmap`).

## 1. The Current State: `ffi_bridge.rs` (Named Pipes)

Currently, `ffi_bridge.rs` is responsible for handling commands between the Frontend (Tauri, CLI, Electron) and the Backend (Rust cluaiz Engine). 

**How it works:**
- It spawns a Windows Named Pipe server at `\\.\pipe\cluaiz_engine_pipe`.
- The frontend connects to this pipe and sends JSON payloads (e.g., `{"action": "GET_SETTINGS"}`).
- The backend parses the JSON and streams back text responses or Server-Sent Events (SSE) equivalent token streams.

**The Limitation:**
Named Pipes are fantastic for small payloads (JSON configs, chat strings, commands). However, if you are building an extension or a native frontend that needs to pass **a 10GB Tensor / Matrix** or raw image buffers directly to the neural engine, using a pipe requires:
1. Serialization to string/bytes.
2. Writing into the OS pipe buffer (Copy 1).
3. Rust reading from the OS pipe buffer (Copy 2).
4. Rust allocating memory for the tensor.

This "Double Copy" is fatal for LLM performance and destroys VRAM/RAM bandwidth.

## 2. The Solution: Zero-Copy FFI via Memory Mapped Files (mmap)

To achieve true native performance ("Silicon Truth"), we bypass pipes for heavy data and use **Memory Mapped Files** combined with **C-compatible FFI (Foreign Function Interface)**.

### Concept

Instead of sending the data *through* the pipe, we put the data in a shared memory block, and only send the *pointer/address* of that block through FFI or the pipe.

### Step-by-Step Implementation for cluaiz

#### Step 1: Frontend allocates Shared Memory
In your C++ or Tauri (Rust) frontend, you create a shared memory block.
**On Windows (C++):**
```cpp
HANDLE hMapFile = CreateFileMapping(
    INVALID_HANDLE_VALUE,    // use paging file
    NULL,                    // default security
    PAGE_READWRITE,          // read/write access
    0,                       // maximum object size (high-order DWORD)
    1024 * 1024 * 1024,      // maximum object size (low-order DWORD) = 1GB
    "cluaizSharedTensor"    // name of mapping object
);

void* pBuf = MapViewOfFile(hMapFile, FILE_MAP_ALL_ACCESS, 0, 0, 1024 * 1024 * 1024);
// Write tensor data directly to pBuf...
```

**On Linux/Mac (C++):**
```cpp
int fd = shm_open("/cluaiz_shared_tensor", O_CREAT | O_RDWR, 0666);
ftruncate(fd, 1024 * 1024 * 1024);
void* pBuf = mmap(0, 1024 * 1024 * 1024, PROT_WRITE, MAP_SHARED, fd, 0);
// Write tensor data directly to pBuf...
```

#### Step 2: Inform cluaiz Engine via FFI
You expose a C-compatible FFI function in cluaiz (`libcluaiz.so` or `cluaiz.dll`).

**In cluaiz Rust Backend:**
```rust
#[no_mangle]
pub extern "C" fn cluaiz_inject_tensor(shared_mem_name: *const std::os::raw::c_char, size: usize) -> i32 {
    let name = unsafe { std::ffi::CStr::from_ptr(shared_mem_name).to_string_lossy().into_owned() };
    
    #[cfg(windows)]
    {
        // Open the memory map that the frontend created
        // ... (winapi calls to OpenFileMapping and MapViewOfFile)
        // Now Rust has a raw pointer `*mut u8` pointing to the exact same physical RAM!
    }
    
    // Inject the raw pointer directly into llama.cpp / Foundry VRAM orchestrator.
    // ZERO COPIES MADE.
    
    0 // Success
}
```

#### Step 3: Fast Interfacing
Once the memory is mapped, the Frontend and Backend can read/write to the same RAM address simultaneously. 
You can use `ffi_bridge.rs` (Named Pipe) simply as a **synchronization signal**:
- Frontend writes data to RAM.
- Frontend sends short JSON pipe message: `{"action": "PROCESS_TENSOR", "name": "cluaizSharedTensor"}`
- Rust reads memory, processes it via GPU, writes output back to the same shared memory.
- Rust sends short JSON pipe message: `{"status": "DONE"}`
- Frontend reads output directly from RAM.

## Summary

- **`ffi_bridge.rs` (Named Pipes):** Use for commands, config, streaming text tokens, CEL execution commands.
- **FFI (`extern "C"`) + `mmap`:** Use for native Extensions, Plugins, Audio buffers, Vision buffers, and Tensor weight offloading to guarantee Zero-Copy latency.
