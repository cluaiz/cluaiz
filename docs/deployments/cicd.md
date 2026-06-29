# Release Workflows

The cluaiz release pipeline enforces zero-crash standards, compiling and testing binary compatibility for every target platform on every update.

---

## The Build Factories

The build process is managed in GitHub Actions, split into four isolated workflow engines:

### 1. CPU Kernels Workflow (`inference-kernel.yml`)
*   **Action:** Cross-compiles CPU vector engines for AVX512, AVX2, and Neon instructions.
*   **Compilation Engine:** Employs Docker-based `cross` compilers to compile Aarch64 and ARMv7 binaries.
*   **Releases:** Packs compiled libraries and uploads them to official `kernel-v*` release tags.

### 2. Accelerator Drivers Workflow (`inference-driver.yml`)
*   **Action:** Compiles specialized GPU/NPU backend matrices (CUDA, Metal, Vulkan, ROCm, OpenVINO).
*   **Registry Sync:** On a successful build, the runner automatically updates version identifiers inside `registry.json` placeholders to enable automated OTA (over-the-air) driver updates.
*   **Releases:** Dynamically uploads accelerator dynamic libraries to official `driver-v*` release tags.

### 3. Application Builds (`cluaiz-cli.yml` & `cluaiz-engine.yml`)
*   **Action:** Compiles the final user CLI TUI and Rust engine.
*   **Testing Suite:** Runs complete unit test suites and validates Axum endpoint integrity before release approval.

### 4. Sandbox Auditing (`modules.yml`)
*   **Action:** Tests runtime module injection security to ensure sandboxed operations are safe from unauthorized host access.
