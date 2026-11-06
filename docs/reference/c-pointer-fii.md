# C Library and FFI Memory Rules

The release packages do not include this library yet: build it with `cargo build --release -p cluaiz-ffi`.

The Engine can run inside another program as a C library: `cluaiz_cep.dll` (Windows),
`libcluaiz_cep.so` (Linux), `libcluaiz_cep.dylib` (macOS). The header is
[`ffi/include/cep.h`](../../ffi/include/cep.h), the code [`ffi/src/lib.rs`](../../ffi/src/lib.rs).

---

## 1. The two functions

```c
char *cep_call(const char *request_json);
void  cep_free(char *reply);
```

| Function | In | Out |
|---|---|---|
| `cep_call` | A NUL-terminated UTF-8 JSON-RPC 2.0 request or batch, the same as the body of `POST /v1/cep` | A NUL-terminated UTF-8 JSON reply, or `NULL` when the request held only notifications |
| `cep_free` | A pointer returned by `cep_call` (or `NULL`) | — |

## 2. Memory rules

1. **The request stays yours.** The Engine copies `request_json` during the call and never keeps
   or frees it.
2. **The reply is the Engine's.** Free every non-NULL reply with `cep_free`, exactly once. Never
   free it with `free()` or your language's allocator: it was allocated by Rust.
3. **`NULL` is safe.** `cep_call(NULL)` returns an error reply; `cep_free(NULL)` does nothing.
4. **No callbacks, no shared buffers.** Nothing crosses the boundary except the two strings. Long
   work returns a task handle that you poll with `tasks.get` / `tasks.result`.

## 3. Threads and errors

- `cep_call` blocks until the reply is ready. Several threads may call at once.
- A thread that is already running an async runtime gets an error reply instead of a deadlock.
- A panic inside the Engine becomes a JSON-RPC `-32603` reply. It never unwinds into your
  program.
- The first call starts the Engine with `CLUAIZ_HOME` (or the default home). It does not write
  `session.token`, so a `cluaiz serve` on the same home keeps working.

## 4. Examples

### C

```c
#include "cep.h"
#include <stdio.h>

int main(void) {
    char *reply = cep_call("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"system.info\"}");
    if (reply) {
        puts(reply);
        cep_free(reply);
    }
    return 0;
}
```

### Python (ctypes)

```python
import ctypes, json

lib = ctypes.CDLL("./cluaiz_cep.dll")
lib.cep_call.argtypes = [ctypes.c_char_p]
lib.cep_call.restype = ctypes.c_void_p      # keep the raw pointer so it can be freed
lib.cep_free.argtypes = [ctypes.c_void_p]

ptr = lib.cep_call(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "rpc.discover"}).encode())
try:
    reply = json.loads(ctypes.string_at(ptr).decode())
finally:
    lib.cep_free(ptr)
```

On Windows with a CUDA build, add the CUDA `bin` folder with `os.add_dll_directory` first; see
[ffi/README.md](../../ffi/README.md).

## 5. Measured

`test/cep/ffi_python.py` (Python ctypes): 12/12 checks pass; 10,000 calls added no measurable
memory; p95 0.11 ms per call.

## 6. Plugins do not use this boundary

Installed plugins are not loaded into the Engine's memory. A plugin runs as a separate child
process: the payload is passed as an argument and its stdout is the result
(`agents/src/plugins/executor.rs`), with your user's rights. Only a `.wasm` plugin is closed in (`agents/src/plugins/wasm.rs`).
