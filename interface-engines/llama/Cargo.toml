[package]
name = "llama"
version = "0.1.0"
edition = "2021"
authors = ["Cluaiz Technologies"]
build = "build.rs"

[lib]
name = "cluaiz_llama"
crate-type = ["cdylib", "rlib", "staticlib"]

[dependencies]
# ── Sovereign LLaMA Backend (Direct C++ Bridge) ──
anyhow = "1.0"
tracing = "0.1"
cluaiz-shared = { path = "../../inference-engine/engines/cluaiz-shared" }
tokenizers = "0.22.2"
libloading = "0.8"
tokio = { version = "1.37", features = ["full"] }
neural-core = { path = "../neural_core" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
libc = "0.2"

[build-dependencies]
cc = "1.0"
bindgen = "0.69"
cmake = "0.1"

[features]
# ── Silicon Driver Switches ──
default = ["cuda"]

# High-Performance GPUs
cuda = []       # NVIDIA GPU
metal = []      # Apple GPU
vulkan = []     # Cross-GPU (Vulkan SDK)
rocm = []       # AMD GPU (ROCm)

# NPU & Specialized Hardware (Hyper-Silicon)
openvino = []   # Intel NPU / Integrated GPU
qnn = []        # Qualcomm Snapdragon NPU (Android)
sycl = []       # Intel GPU (oneAPI)
cann = []       # Huawei Ascend NPU
arm_neon = []   # ARM CPU optimization (Raspberry Pi / Mobile)

[[bin]]
name = "test_llama"
path = "src/bin/test_llama.rs"

