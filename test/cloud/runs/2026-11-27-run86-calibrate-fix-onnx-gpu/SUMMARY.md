# Run 86: calibrate crash fixed, ONNX runs on CUDA on Linux

Date: 2026-11-27. Machine: cluaiz-l4b (Ubuntu 24.04, NVIDIA L4 22.5 GB, 4 cores / 8 threads, 31 GB RAM).
Isolated `CLUAIZ_HOME=~/run86/home`, port 8016. Debug build of `cmd` with the two fixes below.
Follows Run 84, where `cluaiz calibrate` crashed and ONNX on CUDA was not proven.

## Fix 1: `cluaiz calibrate` no longer crashes

Cause: the CUDA download uses reqwest's blocking client, which owns a tokio runtime; it was dropped on
an async worker ("Cannot drop a runtime in a context where blocking is not allowed", exit 101).
Change: `cmd/src/main.rs` runs the calibrate step in `tokio::task::spawn_blocking`.
Result (`raw/calibrate.txt`): exit 0 in 3 min 32 s; cudart, cuBLAS, cuFFT and cuDNN 9.13.0.50 (cuda13)
installed into `home/bin` (1.4 GB), then the hardware profile printed.

## Fix 2: cuDNN parts are found in `home/bin`

Before the fix, with CUDA registered: TTS request 1 failed with HTTP 500 "CUDNN_STATUS_SUBLIBRARY_LOADING_FAILED"
(ConvTranspose) and "SUBLIBRARY_UNAVAILABLE" (RNN); requests 2 and 3 answered (200) in 4.47 s and 1.39 s.
Setting `LD_LIBRARY_PATH` did not help (`raw/requests_ldpath.txt`, `raw/before_fix_errors.txt`).
`LD_DEBUG=files` showed the cause: `libcudnn.so.9` opens its parts by full version
(`libcudnn_graph.so.9.13.0`, through its `$ORIGIN` runpath). The Engine keeps only the short names, so
the lookup missed `home/bin` and loaded a second, system copy of cuDNN from `/lib/x86_64-linux-gnu`.
On a machine without system cuDNN it would fail outright.
Change: `engine/onnx/src/cudnn_driver.rs` links each `libcudnn*.so.9` under its full version
(`libcudnn_graph.so.9.13.0 -> libcudnn_graph.so.9`); symlinks, no extra disk. Also repairs old installs.

## Results after both fixes (`raw/requests.txt`, `raw/serve_onnx_lines.txt`)

| Request | HTTP | Time |
|---|---|---|
| TTS Kokoro 82M UINT8, 1st | 200 | 4.59 s (includes load) |
| TTS 2nd / 3rd | 200 / 200 | 0.58 s / 0.57 s |
| STT Moonshine tiny INT8 on the TTS audio, 1st / 2nd | 200 / 200 | 2.20 s / 0.41 s |

- `CUDAExecutionProvider` registered for both models; 0 cuDNN errors in the log.
- GPU memory of the server after TTS: 728 MiB.
- STT text: "Hello, this is a short speech test from Backlays Engine on A Linux POP." (input sentence:
  "...from the Cluaiz engine on a Linux GPU."): tiny model accuracy, not an Engine fault.
- Unit tests `onnx-engine` `cudnn_driver`: 12 passed, 0 failed, 2 ignored (new test included).

## Notes and not done

- TTS answers JSON with base64 WAV (`audio_data`); the test decodes it before STT. `response_format`
  is not an accepted field (HTTP 400).
- Windows and the CUDA 12 variant not retested. CPU-vs-GPU speed of the same request not measured here.
- Rerun: `run.sh` on the VM after `cluaiz calibrate` with the same `CLUAIZ_HOME`.
