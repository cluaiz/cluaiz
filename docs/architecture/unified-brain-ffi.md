# C Library (FFI) and GPU Backend Loading

## 1. The Engine as a C library

`cluaiz_cep.dll` / `libcluaiz_cep.so` / `libcluaiz_cep.dylib` runs the Engine inside another
program. It exports two functions:

```c
char *cep_call(const char *request_json);  /* JSON-RPC 2.0 in, the reply out */
void  cep_free(char *reply);               /* frees a reply */
```

The messages are the same as `POST /v1/cep`. The first call starts the Engine with `CLUAIZ_HOME`.
A panic inside the Engine becomes a `-32603` reply and never unwinds into the host. Long work
returns a task handle (`tasks.get`, `tasks.result`).

Details, building and Windows DLL loading: [ffi/README.md](../../ffi/README.md).

Measured (Python ctypes, `test/cep/ffi_python.py`): 12/12 checks pass; 10,000 calls added no
measurable memory; p95 0.11 ms per call.

## 2. GPU backends are separate libraries

The `cluaiz` executable is built without CUDA or Vulkan. llama.cpp's GPU backends are shared
libraries (`ggml-cuda`, `ggml-vulkan`, …) installed next to the executable by the GPU packs.

1. At start, llama.cpp loads every backend library in the executable's folder
   (`ggml_backend_load_all_from_path`, `engine/llama/src/native/core.rs`).
2. A backend whose driver is missing does not load; the others still work.
3. With no GPU backend, the CPU backend runs (AVX2, AVX-512 or NEON).

The installer picks the pack from the GPU it finds: NVIDIA → CUDA 12 or 13 (by driver version),
AMD or Intel → Vulkan, otherwise CPU only. A GPU pack must come from the same llama.cpp commit as
the core build. See [cicd.md](../deployments/cicd.md).
