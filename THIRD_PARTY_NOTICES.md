# Third-Party Notices

Cluaiz is licensed under the Apache License, Version 2.0 (see [LICENSE](LICENSE)).
This document provides attribution and license notices for third-party open-source components and on-demand runtime redistributables used by Cluaiz.

---

## 1. llama.cpp
- **License:** MIT License
- **Copyright:** Copyright (c) 2023-2024 The ggml authors

## 2. ONNX Runtime
- **License:** Apache License, Version 2.0
- **Copyright:** Copyright (c) Microsoft Corporation. All rights reserved.

## 3. LMDB (Lightning Memory-Mapped Database)
- **License:** OpenLDAP Public License
- **Copyright:** Copyright 2011-2024 Howard Chu, Symas Corp. All rights reserved.

## 4. Rust Ecosystem Crates & Wasmtime
- **License:** MIT / Apache-2.0 (Wasmtime under Apache-2.0 with LLVM Exceptions)

## 5. Pronunciation Dictionaries & Unicode CLDR (Downloaded on Demand)
- **ipa-dict:** https://github.com/open-dict-data/ipa-dict (MIT / CC BY / CC BY-SA / Unicode; pinned in `engine/onnx/src/audio/tts/data/lexicon_sources.json`).
- **Unicode CLDR:** https://github.com/unicode-org/cldr-json (Unicode License v3, `SPDX-License-Identifier: Unicode-3.0`).

## 6. NVIDIA CUDA Runtime & cuDNN Redistributables (Downloaded on Demand, Not Bundled in Source)
When an NVIDIA GPU is detected on a user's machine and the ONNX Runtime CUDA Execution Provider (`onnxruntime_providers_cuda`) is active, Cluaiz may download official NVIDIA redistributable runtime archives (`cuDNN 9`, `cuda_cudart`, `libcublas`, `libcufft`, and on Linux `libcurand`) directly from NVIDIA's public redistributable CDN (`https://developer.download.nvidia.com/compute/`) into the user's local `<cluaiz_home>/bin/` directory, verifying each archive's byte size and SHA-256 digest against `engine/onnx/src/data/cudnn_sources.json`.

These runtime shared libraries are proprietary software of NVIDIA Corporation and are governed by:
- **NVIDIA CUDA Toolkit End User License Agreement (EULA):** https://docs.nvidia.com/cuda/eula/index.html
- **NVIDIA cuDNN Software License Agreement (SLA):** https://docs.nvidia.com/deeplearning/cudnn/sla/index.html

Copyright (c) NVIDIA Corporation. All rights reserved.
