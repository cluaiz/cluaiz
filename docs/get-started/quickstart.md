# Quickstart Guide

This guide details the deployment pathways to install, bootstrap, and execute cluaiz natively on your workstation. The runtime can be deployed via a single-line automated installer or compiled manually from source.

cluaiz runs with zero virtualized overhead or local containerization: **No Docker. No Python. Pure Silicon.**

---

## ⚡ 1. Single-Command Native Installation

The recommended method to deploy the node is using the official single-line automated installation scripts. These scripts analyze your host operating system, configure local workspaces, fetch target architecture libraries, and register path variables automatically.

### 💻 Windows Installation

Execute this command inside a standard or Administrator **PowerShell** prompt:

```powershell
powershell -ExecutionPolicy Bypass -Command "irm https://cluaiz.com/install.ps1 | iex"
```

*   **Bypass Flag (`-ExecutionPolicy Bypass`):** Instructs PowerShell to bypass local script signing policies temporarily to execute the installer.
*   **Request Method (`irm` / `Invoke-RestMethod`):** Downloads the raw PowerShell script from the secure gateway.
*   **Execution block (`iex` / `Invoke-Expression`):** Runs the downloaded installer script directly inside the active terminal process.
*   **Behind the Scenes:** The installer maps execution paths, creates `~/.cluaiz/workspace`, downloads the Windows-compatible baseline kernels (`cluaiz-kernel.exe`), and appends the binary folder to your user system path environment variables.

---

### 🐧 Linux & macOS Installation

Execute this command inside any **Bash**-compliant terminal:

```bash
curl -fsSL https://cluaiz.com/install.sh | bash
```

*   **Silent Secure Flags (`-fsSL`):** Sets curl to fail silently on server errors, suppress progress bars, allow redirects, and connect securely over SSL.
*   **Piped Shell (`| bash`):** Directs the downloaded shell instructions directly to the bash shell interpreter.
*   **Behind the Scenes:** 
    *   **On macOS:** The script checks for Apple Silicon (ARM64) or legacy Intel (x86_64) configurations to fetch the optimized dynamic framework files.
    *   **On Linux:** The installer audits CPU SIMD levels (AVX512, AVX2) and selects the corresponding optimized compiler binary.
    *   It populates `~/.cluaiz/workspace/` and appends the execution path to your shell configuration profile (`.bashrc` / `.zshrc`).

---

## 🛠️ 2. Manual From-Source Compilation

For developers and contributors who want to configure custom target compilation flags, cluaiz can be built manually from source:

### System Prerequisites
Ensure your local machine has the following tools installed and registered in your PATH:
*   **Compiler:** Rust Toolchain (v1.75 or later).
*   **Linkers:** C++ build compilers (GCC/Clang on Unix/macOS, MSVC C++ Build Tools on Windows).

### Compilation Sequence
1.  **Clone the Repository:**
    ```bash
    git clone https://github.com/cluaiz/cluaiz.git
    cd cluaiz
    ```

2.  **Compile the Core Workspace:**
    ```bash
    cargo build --release --bin cluaiz
    ```
    *Note: The `--release` flag compiles the Rust codebase with maximum level-3 loop optimizations and strips debugger symbols to ensure maximum execution speeds.*

3.  **Verify Binary Generation:**
    Verify that the compiled binary is generated under the release sub-directory:
    ```bash
    ./target/release/cluaiz --version
    ```

---

## 🕯️ 3. Post-Installation: The First Boot

Once installed, execute `cluaiz` in your terminal to begin the node initialization sequence:

```bash
cluaiz
```

### The Onboarding Sequence

On the first run, the interface executes a structured terminal onboarding process:

1.  **Workspace Seeding:** The bootstrapper generates persistent configuration profiles (`IDENTITY.md`, `USER.md`, and `SOUL.md`) under `~/.cluaiz/workspace/`.
2.  **Sentinel Generation:** The system mounts a temporary `.ignition_lock` file in the configuration sandbox. If the terminal process crashes or is closed during this phase, subsequent boots detect the lock file and resume setup at the exact step left off.
3.  **Operator Profile:** The interface prompts for your operator username and generation purpose (`RESEARCH` for deep-quant optimization vs. `PRODUCTION` for maximum token-per-second throughput).
4.  **Hardware Audit:** The engine queries core processors to determine physical thread pools, local VRAM/RAM pools, and dynamic accelerator driver support (CUDA, Metal).
5.  **Activation:** Upon successful compilation and validation of config state, the `.ignition_lock` file is permanently deleted, and the terminal launches the active TUI Dashboard.
