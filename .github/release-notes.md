## Install

**Linux and macOS**

```sh
curl -fsSL https://cluaiz.com/install.sh | bash
```

**Windows** (Command Prompt or PowerShell)

```powershell
powershell -c "irm https://cluaiz.com/install.ps1 | iex"
```

The installer picks the files for your machine and checks them against `SHA256SUMS.txt`.

## Files

| File | Who needs it |
|---|---|
| `cluaiz-<version>-<os>.zip` / `.tar.gz` | Everyone. The CLI, API and engine; runs every model on the CPU and picks the best CPU code for your processor. |
| `…-llama-cuda13` + `…-cudart13` + `…-onnx-cuda13` | NVIDIA RTX 30 / 40 / 50 with a CUDA 13 driver (580+); RTX 20 and other Turing GPUs with a driver for CUDA 13.4 or newer. |
| `…-llama-cuda12` + `…-cudart12` + `…-onnx-cuda12` | Other NVIDIA GPUs (GTX 900 / 10 series, Turing on an older driver). |
| `…-llama-vulkan` | AMD, Intel Arc and other GPUs with a Vulkan driver (GGUF models). |

`llama-*` files are the GPU drivers for GGUF models, `onnx-*` for ONNX models (speech, transcription). The NVIDIA cuDNN library that `onnx-cuda*` needs is downloaded by the engine from NVIDIA on first use.

To install by hand, unpack the core and the GPU files into the same folder. A GPU driver whose GPU or NVIDIA / AMD driver is missing is skipped, and models run on the CPU.
