## Install

**Linux**

```sh
curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.sh | sh
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.ps1 | iex
```

The installer picks the files for your machine and checks them against `SHA256SUMS.txt`.

## Files

| File | Who needs it |
|---|---|
| `cluaiz-<version>-<os>.zip` / `.tar.gz` | Everyone. The CLI, API and engine; runs every model on the CPU, picks the best CPU code for your processor. |
| `…-llama-cuda12` + `…-cudart12` | NVIDIA GPUs (GTX 900 series and newer) for GGUF models. Needs the NVIDIA driver. |
| `…-onnx-cuda` | NVIDIA GPUs for ONNX models (speech, transcription). |
| `…-llama-vulkan` | AMD, Intel and other GPUs with a Vulkan driver, for GGUF models. |

To install by hand, unpack the core and any GPU pack into the same folder. A GPU pack whose driver or GPU is missing is skipped, and models run on the CPU.
