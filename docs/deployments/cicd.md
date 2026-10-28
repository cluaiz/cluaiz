# Release Workflow

[`.github/workflows/release.yml`](../../.github/workflows/release.yml) builds and publishes Cluaiz: a **core** that runs everything on the CPU, plus the **GPU drivers** the core loads when they sit beside it. The drivers come from two workflows that `release.yml` calls, so every file of a version lands on **one release page**:

- [`cluaize-llama-driver.yml`](../../.github/workflows/cluaize-llama-driver.yml): GGUF drivers, llama.cpp's CUDA 12, CUDA 13 and Vulkan backends plus the NVIDIA CUDA runtime.
- [`cluaize-onnx-driver.yml`](../../.github/workflows/cluaize-onnx-driver.yml): ONNX drivers, ONNX Runtime's CUDA 12 and CUDA 13 providers.

---

## When it runs

Only when started by hand. Pushes and pull requests never start a build.

| Event | What runs |
|---|---|
| **Run workflow** on `Release` (manual, with a version such as `0.1.0`) | The core for every platform, the smoke test, both driver workflows, and the GitHub release `v<version>` with `SHA256SUMS.txt`. |
| **Run workflow** on a driver workflow | Only those drivers, as run artifacts (no release). |

## Jobs

| Job | Runner | Builds | Output |
|---|---|---|---|
| `core (win-x64)` | `windows-2022` | `cargo build -p cmd --release --locked --no-default-features` | `cluaiz-<v>-win-x64.zip` |
| `core (linux-x64)` | `ubuntu-24.04` | same | `cluaiz-<v>-linux-x64.tar.gz` |
| `core (linux-arm64)` | `ubuntu-24.04-arm` | same (experimental: a failure does not block the release) | `cluaiz-<v>-linux-arm64.tar.gz` |
| `core (mac-arm64)` | `macos-14` | same with `--features metal` (experimental) | `cluaiz-<v>-mac-arm64.tar.gz` |
| `llama cuda12` / `cuda13` (win-x64) | `windows-2022` + CUDA 12.4 / 13.4 | CMake target `ggml-cuda` only | `…-win-x64-llama-cuda12.zip` + `…-cudart12.zip`, same for 13 |
| `llama vulkan` (win-x64) | `windows-2022` + Vulkan SDK | CMake target `ggml-vulkan` only | `…-win-x64-llama-vulkan.zip` |
| `llama cuda12` / `cuda13` (linux-x64) | `nvidia/cuda:12.8.1` / `13.4.1-devel-ubuntu24.04` | CMake target `ggml-cuda` only | `…-linux-x64-llama-cuda12.tar.gz` + `…-cudart12.tar.gz`, same for 13 |
| `llama vulkan` (linux-x64) | `ubuntu-24.04` | CMake target `ggml-vulkan` only | `…-linux-x64-llama-vulkan.tar.gz` |
| `onnx cuda12` / `cuda13` (win-x64, linux-x64) | `ubuntu-24.04` | nothing compiled: Microsoft's ONNX Runtime GPU release of the version in `Cargo.lock`, checked against GitHub's SHA-256 | `…-onnx-cuda12.*`, `…-onnx-cuda13.*` |
| `release` | `ubuntu-24.04` | — | the GitHub release with every file that built and `SHA256SUMS.txt` |

GGUF drivers are built from the same pinned `engine/llama/llama.cpp` fork commit as the core, so they match the core's `ggml-base` library. ONNX drivers are the same ONNX Runtime version the core links (read from `ort-sys` in `Cargo.lock`); NVIDIA cuDNN and the CUDA runtime they need are downloaded by the engine on first GPU use. A driver job that fails leaves its file out of the release; the core still ships.

## Smoke test

Each core is unpacked into an empty folder and run with a fresh `CLUAIZ_HOME` on a runner that has no GPU and no GPU runtime ([`.github/scripts/smoke_test.sh`](../../.github/scripts/smoke_test.sh)):

1. `cluaiz --version` and `cluaiz status` start.
2. `cluaiz pull` downloads a small GGUF chat model.
3. `cluaiz run` answers a prompt; the log must show the CPU backend loaded from the core's folder.

## Making a release

1. GitHub → **Actions** → **Release** → **Run workflow**.
2. Enter the version (`0.1.0`) and keep **pre-release** ticked for test builds.
3. When it finishes, the release page lists the files and their checksums. Running it again with the same version replaces the files.

The CUDA jobs take the longest (the CUDA backend is compiled for every supported GPU generation).

## Speed and reproducibility

- `Cargo.lock` is committed and CI builds with `--locked`, so dependencies only change when the lock file does.
- Rust dependencies are cached per platform (`Swatinem/rust-cache`); on Linux and macOS, llama.cpp's C++ build is cached with `ccache`.

## Workflows in `.github/workflows`

| File | Purpose |
|---|---|
| `release.yml` | Builds and tests the core, calls the driver workflows, publishes one release page. |
| `cluaize-llama-driver.yml` | GGUF drivers (llama.cpp CUDA 12 / 13, Vulkan) and the NVIDIA CUDA runtime. |
| `cluaize-onnx-driver.yml` | ONNX drivers (ONNX Runtime CUDA 12 / 13 providers). |

The earlier split workflows (`cluaize-cmd`, `-engine`, `-kernel-*`, `publish-registry`), their registry scripts and Release Drafter (it ran on every push but builds its changelog only from pull requests, which this repository does not use) were removed; they built package names that no longer exist and never produced a release.
