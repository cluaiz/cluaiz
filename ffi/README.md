<!-- name: ffi
description: The Engine as a C library (crate cluaiz-ffi) and how to build it; releases do not include it yet. -->

# The Engine as a C library (`ffi/`, crate `cluaiz-ffi`)

![cep_call takes a JSON-RPC request and returns the reply from the same CEP methods as /v1/cep; cep_free frees it](../docs/assets/engine/ffi-call.png)

The release packages do not include this library yet: build it with `cargo build --release -p cluaiz-ffi`.

`cluaiz_cep.dll` / `libcluaiz_cep.so` / `libcluaiz_cep.dylib` runs the Cluaiz Engine inside another program. The library exports two functions (header: [`include/cep.h`](include/cep.h)):

```c
char *cep_call(const char *request_json);  /* JSON-RPC 2.0 in, the reply out (NULL for notifications only) */
void  cep_free(char *reply);               /* frees a reply */
```

The messages are the same as `POST /v1/cep`. Ask `rpc.discover` for the methods.

## How it runs

- **Start-up.** The first call starts the Engine with `CLUAIZ_HOME` (or the default home). It uses the same start-up code as `cluaiz serve`, but does **not** write `session.token`, so a daemon running on the same home keeps working.
- **Who is calling.** The caller is the owner: the calls come from the host process, under the same OS user.
- **Threads.** `cep_call` blocks until the answer. Several threads may call at once. A thread that is running an async runtime gets an error reply instead of a deadlock.
- **Panics.** A panic inside the Engine becomes a `-32603` reply. It never unwinds into the host.
- **Long work.** It returns a task handle, which the host polls with `tasks.get` / `tasks.result`.

## Building

Build it together with the CLI, so both share one GPU backend build:

```bash
cargo build --release -p cmd -p cluaiz-ffi
```

The library is not named `cluaiz`, because its debug files would clash with `cluaiz.exe`'s in `target/release`.

## Loading on Windows

The library needs the same runtime DLLs as `cluaiz.exe`. For a CUDA build that includes cuBLAS from `CUDA_PATH\bin`. Python 3.8 and later does not search `PATH` for a DLL's dependencies, so add the folder first:

```python
os.add_dll_directory(os.path.join(os.environ["CUDA_PATH"], "bin"))
lib = ctypes.CDLL(r"...\cluaiz_cep.dll")
```

## Tests

- `cargo test --release -p cluaiz-ffi`: the panic guard and NULL handling.
- `python test/cep/ffi_python.py --lib <dll> --home <isolated home>`: the whole library through ctypes.

## Results

`cluaiz-ffi` depends on `cluaiz-api`, so its tests build the native backends; not run on 2026-11-21.
The ctypes journey `test/cep/ffi_python.py` has no recorded run in `test/cloud/runs/`.

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| The library is not in the release packages | embedding the Engine needs a local build | a release job for `cluaiz_cep` |
| No recorded run of `ffi_python.py` | the C ABI path has no checked-in result | one run with an isolated home |
