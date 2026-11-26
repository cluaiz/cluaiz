# Third-Party Notices

Cluaiz is licensed under the Apache License, Version 2.0 (see [LICENSE](LICENSE)).
This document lists the third-party components that Cluaiz builds in, ships in its release packages, or downloads
on demand, with their licenses.

---

## 1. llama.cpp and ggml (built in; `ggml*`, `llama*`, `mtmd*` libraries in every package)
- **License:** MIT License
- **Copyright:** Copyright (c) 2023-2026 The ggml authors
- **Source:** https://github.com/ggml-org/llama.cpp (Cluaiz builds a fork pinned as the submodule `engine/llama/llama.cpp`)
- Includes the CUDA, Vulkan and Metal backends of ggml (same license).

## 2. ONNX Runtime (`onnxruntime*` libraries in the packages)
- **License:** MIT License
- **Copyright:** Copyright (c) Microsoft Corporation
- **Source:** https://github.com/microsoft/onnxruntime (prebuilt binaries fetched by the `ort` crate)

## 3. DirectML (`DirectML.dll`, Windows packages)
- **License:** Microsoft Software License Terms, Microsoft DirectX Machine Learning (DirectML), the terms of the
  `Microsoft.AI.DirectML` package: https://www.nuget.org/packages/Microsoft.AI.DirectML/1.15.4/License
- **Copyright:** Copyright (c) Microsoft Corporation. All rights reserved.
- Shipped inside Cluaiz as the runtime of ONNX Runtime's DirectML execution provider, as the terms allow for
  applications; not offered on its own.

## 4. NVIDIA CUDA runtime and cuDNN redistributables (GPU packs and on-demand downloads)
- **CUDA packs** (`cudart12` / `cudart13` release archives): `cudart`, `cublas` and `cublasLt` runtime libraries,
  copied from the NVIDIA CUDA Toolkit during the release build.
- **On demand:** when an NVIDIA GPU is found and ONNX Runtime's CUDA execution provider is used, Cluaiz may
  download NVIDIA's redistributable archives (`cuDNN 9`, `cuda_cudart`, `libcublas`, `libcufft`, and on Linux
  `libcurand`) from `https://developer.download.nvidia.com/compute/` into `<cluaiz_home>/bin/`, checking each
  archive's size and SHA-256 against `engine/onnx/src/data/cudnn_sources.json`.

These libraries are proprietary software of NVIDIA Corporation, governed by:
- **NVIDIA CUDA Toolkit End User License Agreement:** https://docs.nvidia.com/cuda/eula/index.html
- **NVIDIA cuDNN Software License Agreement:** https://docs.nvidia.com/deeplearning/cudnn/sla/index.html

Copyright (c) NVIDIA Corporation. All rights reserved.

## 5. LLVM OpenMP runtime (`libomp.dll`, Windows packages)
The OpenMP runtime of the LLVM project, loaded by the ggml libraries that are built with clang on Windows (as in
llama.cpp's own Windows release).
- **License:** Apache License, Version 2.0 with LLVM Exceptions
- **Copyright:** Copyright (c) LLVM Project contributors
- **Source:** https://github.com/llvm/llvm-project/tree/main/openmp

## 6. libgomp (`libgomp.so.1`, Linux packages)
The GNU Offloading and Multi Processing Runtime Library from GCC.
- **License:** GNU General Public License version 3 with the GCC Runtime Library Exception
- **Source:** https://gcc.gnu.org/

## 7. Wasmtime (built in; runs WebAssembly plugins)
- **License:** Apache License, Version 2.0 with LLVM Exceptions
- **Source:** https://github.com/bytecodealliance/wasmtime

## 8. Oniguruma (built in through the `tokenizers` crate)
- **License:** BSD 2-Clause License
- **Copyright:** Copyright (c) 2002-2021 K.Kosako
- **Source:** https://github.com/kkos/oniguruma

## 9. Other Rust crates (built in)
The remaining Rust dependencies (listed in `Cargo.lock`, among them Hugging Face `tokenizers`, `pdf-extract`,
`image`, `zip`, `tokio` and `axum`) are under the MIT License and/or the Apache License, Version 2.0, or similar
permissive licenses. Their license texts are in each crate's source.

## 10. Pronunciation data (built in and downloaded on demand)
- **Unicode CLDR** (data in `engine/onnx/src/audio/tts/data/`, license in `CLDR-LICENSE.txt` there):
  https://github.com/unicode-org/cldr-json, Unicode License v3 (`SPDX-License-Identifier: Unicode-3.0`).
- **ipa-dict** (downloaded on demand at the commit pinned in `engine/onnx/src/audio/tts/data/lexicon_sources.json`):
  https://github.com/open-dict-data/ipa-dict, MIT / CC BY / CC BY-SA / Unicode, depending on the language file.

## Models
Models (GGUF and ONNX) are not part of Cluaiz. They are downloaded by the user from Hugging Face or the Cluaiz
catalogue and keep their own licenses, shown on each model's page.
