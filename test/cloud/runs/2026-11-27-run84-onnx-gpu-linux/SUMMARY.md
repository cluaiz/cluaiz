# Run 84: ONNX on GPU, Linux

Date: 2026-11-27. Machine: cluaiz-l4b (NVIDIA L4). Models: kokoro-82m-v1.0-onnx-UINT8 (TTS), moonshine (STT).

## Results

- `cluaiz calibrate` crashed twice (exit 101, "The application panicked") after 4.3 s
  (`raw/calibrate.txt`, `raw/calibrate_try2.txt`). Open bug: tokio runtime dropped inside a runtime.
- Serve without calibrate: CUDA/cuDNN 9 not present, so the first TTS session ran on CPU while a
  background thread downloaded the NVIDIA libraries (cudart 1.4 MB, cublas 760.4 MB) (`raw/serve_onnx_lines.txt`).
- TTS: 3/3 HTTP 200, 366,994 bytes each, 6.14 s / 1.91 s / 2.00 s. Server alive after each.
- GPU after TTS: one process holding 1,442 MiB (`raw/gpu_apps_after_tts.txt`).
- STT: 2/2 HTTP 415 "Expected request with Content-Type: application/json". Test script sent the wrong
  content type; not an Engine result.

## Open

ONNX on CUDA not proven: calibrate must be fixed, then rerun TTS and STT (with JSON) after the libraries
are installed. Rerun: `run.sh`, `phase2.sh`.
