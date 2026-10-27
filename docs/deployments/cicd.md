# Release Workflow

One workflow, [`.github/workflows/release.yml`](../../.github/workflows/release.yml), builds and publishes Cluaiz. It follows llama.cpp's own release layout: a **core** that runs everything on the CPU, and **GPU packs** that the core loads when they sit beside it.

---

## When it runs

| Event | What runs |
|---|---|
| Push to `main` or a pull request that touches engine / CLI / API code | Core builds for every platform and the smoke test. A broken build is caught here, before a release. |
| **Run workflow** (manual, with a version such as `0.1.0`) | Everything above, the GPU packs, and the GitHub release `v<version>` with `SHA256SUMS.txt`. |

## Jobs

| Job | Runner | Builds | Output |
|---|---|---|---|
| `core (win-x64)` | `windows-2022` | `cargo build -p cmd --release --locked --no-default-features` | `cluaiz-<v>-win-x64.zip`, `cluaiz-<v>-win-x64-onnx-cuda.zip` |
| `core (linux-x64)` | `ubuntu-24.04` | same | `cluaiz-<v>-linux-x64.tar.gz`, `…-onnx-cuda.tar.gz` |
| `core (linux-arm64)` | `ubuntu-24.04-arm` | same (experimental: a failure does not block the release) | `cluaiz-<v>-linux-arm64.tar.gz` |
| `core (mac-arm64)` | `macos-14` | same with `--features metal` (experimental) | `cluaiz-<v>-mac-arm64.tar.gz` |
| `gpu (win-x64-cuda12)` | `windows-2022` + CUDA 12.4 | CMake target `ggml-cuda` only | `…-win-x64-llama-cuda12.zip`, `…-win-x64-cudart12.zip` |
| `gpu (win-x64-vulkan)` | `windows-2022` + Vulkan SDK | CMake target `ggml-vulkan` only | `…-win-x64-llama-vulkan.zip` |
| `gpu (linux-x64-cuda12)` | `nvidia/cuda:12.8.1-devel-ubuntu24.04` | CMake target `ggml-cuda` only | `…-linux-x64-llama-cuda12.tar.gz`, `…-linux-x64-cudart12.tar.gz` |
| `gpu (linux-x64-vulkan)` | `ubuntu-24.04` | CMake target `ggml-vulkan` only | `…-linux-x64-llama-vulkan.tar.gz` |
| `release` | `ubuntu-24.04` | — | the GitHub release with every file that built and `SHA256SUMS.txt` |

GPU packs are built from the same pinned `engine/llama/llama.cpp` fork commit as the core, so they match the core's `ggml-base` library. A GPU job that fails leaves its pack out of the release; the core still ships.

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
| `release.yml` | Builds, tests and publishes (this page). |
| `release-drafter.yml` | Keeps a draft changelog from merged pull requests; `release.yml` publishes that draft when it has the same tag. |

The earlier split workflows (`cluaize-cmd`, `-engine`, `-kernel-*`, `-*-driver`, `publish-registry`) and their registry scripts were removed; they built package names that no longer exist and never produced a release.
