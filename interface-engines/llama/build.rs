use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR").unwrap();
    let mut llama_path = Path::new(&out_dir).join("llama.cpp");

    // ═══════════════════════════════════════════════════════════════
    // PHASE 1: SOVEREIGN CLONE (Prioritize local cached copy)
    // ═══════════════════════════════════════════════════════════════
    let local_cache_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("llama.cpp");
    if local_cache_path.exists() {
        println!("cargo:warning=🔩 Found local cached llama.cpp at {:?}", local_cache_path);
        llama_path = local_cache_path;
    } else if !llama_path.exists() {
        println!("cargo:warning=🔩 Cloning official ggml-org/llama.cpp source...");
        let status = Command::new("git")
            .args(["clone", "--depth", "1", "https://github.com/ggml-org/llama.cpp", llama_path.to_str().unwrap()])
            .status()
            .expect("Failed to clone llama.cpp");
        if !status.success() { panic!("Clone failed"); }
    }

    // ═══════════════════════════════════════════════════════════════
    // PHASE 1.5: SOVEREIGN INJECTION (Apply BFE PTX Injection via Rust)
    // ═══════════════════════════════════════════════════════════════
    let dequantize_cuh_path = llama_path.join("ggml/src/ggml-cuda/dequantize.cuh");
    if dequantize_cuh_path.exists() {
        println!("cargo:warning=💉 Injecting 1-Cycle PTX BFE Assembly into ggml-cuda...");
        let mut content = std::fs::read_to_string(&dequantize_cuh_path).unwrap();
        content = content.replace("\r\n", "\n");
        
        let target_q1 = r#"    // Extract bits: 1 = +d, 0 = -d (branchless)
    const int bit_0 = (x[ib].qs[byte_index_0] >> bit_offset_0) & 1;
    const int bit_1 = (x[ib].qs[byte_index_1] >> bit_offset_1) & 1;"#.replace("\r\n", "\n");
        
        let inject_q1 = r#"    // cluaiz Sovereign: 1-Cycle PTX bfe.u32 Injection
    unsigned int qs_0 = x[ib].qs[byte_index_0];
    unsigned int qs_1 = x[ib].qs[byte_index_1];
    unsigned int bit_0, bit_1;
    asm volatile("bfe.u32 %0, %1, %2, 1;" : "=r"(bit_0) : "r"(qs_0), "r"(bit_offset_0));
    asm volatile("bfe.u32 %0, %1, %2, 1;" : "=r"(bit_1) : "r"(qs_1), "r"(bit_offset_1));"#.replace("\r\n", "\n");

        let target_q4 = r#"    v.x = vui & 0xF;
    v.y = vui >> 4;"#.replace("\r\n", "\n");

        let inject_q4 = r#"    // cluaiz Sovereign: 1-Cycle PTX bfe.u32 Injection
    unsigned int vx_int, vy_int;
    asm volatile("bfe.u32 %0, %1, 0, 4;" : "=r"(vx_int) : "r"((unsigned int)vui));
    asm volatile("bfe.u32 %0, %1, 4, 4;" : "=r"(vy_int) : "r"((unsigned int)vui));
    v.x = vx_int;
    v.y = vy_int;"#.replace("\r\n", "\n");

        if content.contains(&target_q4) || content.contains(&target_q1) {
            let new_content = content.replace(&target_q1, &inject_q1).replace(&target_q4, &inject_q4);
            std::fs::write(&dequantize_cuh_path, new_content).unwrap();
            println!("cargo:warning=✅ BFE PTX Injection Complete!");
        } else {
            println!("cargo:warning=⚠️ BFE PTX already injected or target strings changed.");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // PHASE 1.6: SOVEREIGN KV-CACHE PATCH (Bypass n_pos_per_embd assert for M-RoPE sliding window)
    // ═══════════════════════════════════════════════════════════════
    let kv_cache_cpp_path = llama_path.join("src/llama-kv-cache.cpp");
    if kv_cache_cpp_path.exists() {
        println!("cargo:warning=💉 Patching llama-kv-cache.cpp to support M-RoPE sliding window context shifting...");
        let mut content = std::fs::read_to_string(&kv_cache_cpp_path).unwrap();
        content = content.replace("\r\n", "\n");
        
        let target_add_assert = r#"    GGML_ASSERT(hparams.n_pos_per_embd() == 1 && "seq_add() is only supported for n_pos_per_embd() == 1");"#;
        let patch_add_assert = r#"    // GGML_ASSERT(hparams.n_pos_per_embd() == 1 && "seq_add() is only supported for n_pos_per_embd() == 1"); // cluaiz Sovereign: Bypassed for M-RoPE"#;

        let target_div_assert = r#"    GGML_ASSERT(hparams.n_pos_per_embd() == 1 && "seq_div() is only supported for n_pos_per_embd() == 1");"#;
        let patch_div_assert = r#"    // GGML_ASSERT(hparams.n_pos_per_embd() == 1 && "seq_div() is only supported for n_pos_per_embd() == 1"); // cluaiz Sovereign: Bypassed for M-RoPE"#;

        let target_can_shift = "bool llama_kv_cache::get_can_shift() const {\n    // Step35 uses per-layer RoPE dims; K-shift assumes a single global n_rot.\n    if (model.arch == LLM_ARCH_STEP35) {\n        return false;\n    }\n    if (hparams.n_pos_per_embd() > 1) {\n        return false;\n    }\n    return true;\n}";
        let patch_can_shift = "bool llama_kv_cache::get_can_shift() const {\n    // Step35 uses per-layer RoPE dims; K-shift assumes a single global n_rot.\n    if (model.arch == LLM_ARCH_STEP35) {\n        return false;\n    }\n    if (hparams.n_pos_per_embd() > 1) {\n        // cluaiz Sovereign: allow shifting for M-RoPE models (e.g. Qwen2-VL, Qwen3.5-VL)\n        if (hparams.rope_type == LLAMA_ROPE_TYPE_MROPE || hparams.rope_type == LLAMA_ROPE_TYPE_IMROPE) {\n            return true;\n        }\n        return false;\n    }\n    return true;\n}";

        let mut modified = false;
        if content.contains(target_add_assert) {
            content = content.replace(target_add_assert, patch_add_assert);
            modified = true;
        }
        if content.contains(target_div_assert) {
            content = content.replace(target_div_assert, patch_div_assert);
            modified = true;
        }
        if content.contains(target_can_shift) {
            content = content.replace(target_can_shift, patch_can_shift);
            modified = true;
        }

        if modified {
            std::fs::write(&kv_cache_cpp_path, content).unwrap();
            println!("cargo:warning=✅ M-RoPE KV-Cache Patches Applied!");
        } else {
            println!("cargo:warning=⚠️ M-RoPE KV-Cache Patches already applied or target assertions/checks not found.");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // PHASE 1.7: SOVEREIGN BITNET INTERCEPTION (Stable TQ2_0 Multi-Block)
    // ═══════════════════════════════════════════════════════════════
    let quants_c_path = llama_path.join("ggml/src/ggml-cpu/arch/x86/quants.c");
    if quants_c_path.exists() {
        println!("cargo:warning=💉 Patching x86 quants.c for BitNet TQ2_0 Interception...");
        let mut content = std::fs::read_to_string(&quants_c_path).unwrap();
        content = content.replace("\r\n", "\n");

        let target_sig = r#"void ggml_vec_dot_tq2_0_q8_K(int n, float * GGML_RESTRICT s, size_t bs, const void * GGML_RESTRICT vx, size_t bx, const void * GGML_RESTRICT vy, size_t by, int nrc) {
    assert(nrc == 1);
    UNUSED(nrc);
    UNUSED(bx);
    UNUSED(by);
    UNUSED(bs);

    const block_tq2_0 * GGML_RESTRICT x = vx;
    const block_q8_K  * GGML_RESTRICT y = vy;

    const int nb = n / QK_K;

#if defined(__AVX2__)"#;

        let inject_sig = r#"#ifdef __cplusplus
extern "C" {
#endif
    int cluaiz_fast_ternary_dot(
        const uint8_t * packed_weights,
        const int8_t * activations,
        int32_t * output,
        size_t count
    );
#ifdef __cplusplus
}
#endif

void ggml_vec_dot_tq2_0_q8_K(int n, float * GGML_RESTRICT s, size_t bs, const void * GGML_RESTRICT vx, size_t bx, const void * GGML_RESTRICT vy, size_t by, int nrc) {
    assert(nrc == 1);
    UNUSED(nrc);
    UNUSED(bx);
    UNUSED(by);
    UNUSED(bs);

    const block_tq2_0 * GGML_RESTRICT x = vx;
    const block_q8_K  * GGML_RESTRICT y = vy;

    const int nb = n / QK_K;

    // 🚀 Sovereign Interception: Stable Multi-Block Rust AVX2 Execution
    float cluaiz_total_sum = 0.0f;
    bool cluaiz_success = true;
    for (int i = 0; i < nb; ++i) {
        int32_t cluaiz_out = 0;
        if (cluaiz_fast_ternary_dot((const uint8_t*)x[i].qs, (const int8_t*)y[i].qs, &cluaiz_out, QK_K) == 0) {
            float d_x = GGML_FP16_TO_FP32(x[i].d);
            float d_y = GGML_FP16_TO_FP32(y[i].d);
            cluaiz_total_sum += ((float)cluaiz_out * d_x * d_y);
        } else {
            cluaiz_success = false;
            break;
        }
    }

    if (cluaiz_success) {
        *s = cluaiz_total_sum;
        return;
    }

#if defined(__AVX2__)"#;

        if content.contains("cluaiz_fast_ternary_dot") {
            println!("cargo:warning=⚠️ BitNet Interception already injected.");
        } else if content.contains(target_sig) {
            content = content.replace(target_sig, inject_sig);
            std::fs::write(&quants_c_path, content).unwrap();
            println!("cargo:warning=✅ BitNet TQ2_0 Patches Applied!");
        } else {
            println!("cargo:warning=⚠️ BitNet target signature not found in quants.c!");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // PHASE 2: INDUSTRIAL CMAKE BUILD
    // ═══════════════════════════════════════════════════════════════
    let target_os   = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    let mut config = cmake::Config::new(&llama_path);

    // ── CPU SIMD Vectorization Optimization ──────────────────────────
    let is_ci = env::var("GITHUB_ACTIONS").is_ok();
    
    if is_ci {
        // CI Build: Target baseline optimizations for distribution
        if target_arch == "x86_64" {
            // Enable AVX2 by default for distributed builds (standard for modern hardware)
            config.define("GGML_AVX2", "ON");
            config.define("GGML_FMA", "ON");
            config.define("GGML_F16C", "ON");
            config.define("GGML_AVX", "ON");
            config.define("GGML_AVX512", "OFF");
        }
        config.define("GGML_NATIVE", "OFF");
    } else {
        // Local Build: Detect and compile native instruction set of the host CPU (AVX2, AVX512, AMX, etc.)
        config.define("GGML_NATIVE", "ON");
    }

    config
        .define("LLAMA_BUILD_EXAMPLES", "OFF")
        .define("LLAMA_BUILD_TESTS",    "OFF")
        .define("LLAMA_BUILD_SERVER",   "OFF")
        // FIX 2: Disable llama-bench and all standalone tools.
        // llama-bench pulls in cpp-httplib which links OpenSSL.
        // On macOS x64, OpenSSL is not in the default linker path → LNK error.
        // We only need the static libraries (llama.a, ggml.a, etc.), not any binaries.
        .define("LLAMA_BUILD_TOOLS",    "OFF")
        .define("LLAMA_BUILD_APP",      "OFF")
        .define("LLAMA_STATIC",         "ON")
        .define("BUILD_SHARED_LIBS",    "OFF")
        // Use MultiThreadedDLL (/MD) to match Rust's default dynamic CRT on Windows MSVC,
        // and to avoid LNK2038 mismatch with other C++ libraries like ONNX Runtime (ort) which use /MD.
        // CMAKE_POLICY_DEFAULT_CMP0091=NEW is required for CMake 3.14 to respect CMAKE_MSVC_RUNTIME_LIBRARY
        .define("CMAKE_POLICY_DEFAULT_CMP0091", "NEW")
        .define("CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDLL")
        .profile("Release");

    // ── Apple Platform Alignment ──────────────────────────────────────
    if target_os == "ios" {
        // Apple arch name is "arm64" NOT "aarch64".
        // CARGO_CFG_TARGET_ARCH returns "aarch64" (Rust/Linux naming),
        // but Apple's clang -arch flag only accepts "arm64".
        let apple_arch = if target_arch == "aarch64" { "arm64" } else { &target_arch };
        config.define("CMAKE_SYSTEM_NAME",           "iOS");
        config.define("CMAKE_OSX_SYSROOT",           "iphoneos");
        config.define("CMAKE_OSX_ARCHITECTURES",     apple_arch);
        config.define("CMAKE_OSX_DEPLOYMENT_TARGET", "16.0");
    } else if target_os == "macos" {
        // Sync deployment target with Rust's default macOS linker flag.
        config.define("CMAKE_OSX_DEPLOYMENT_TARGET", "11.0");
    }

    // ═══════════════════════════════════════════════════════════════
    // PHASE 1.8: SOVEREIGN BITNET GGUF COMPATIBILITY (Type 42 -> 35)
    // ═══════════════════════════════════════════════════════════════
    let gguf_cpp_path = llama_path.join("ggml/src/gguf.cpp");
    if gguf_cpp_path.exists() {
        let mut content = std::fs::read_to_string(&gguf_cpp_path).unwrap();
        let target_check = "if (info.t.type < 0 || info.t.type >= GGML_TYPE_COUNT) {";
        let patch = r#"// 🛡️ CLUAIZ SOVEREIGN COMPATIBILITY PATCH
            // Older BitNet models encoded TQ2_0 as 42, but llama.cpp reordered the enums.
            if (info.t.type == 42) {
                info.t.type = GGML_TYPE_TQ2_0;
            }
            if (info.t.type < 0 || info.t.type >= GGML_TYPE_COUNT) {"#;

        if !content.contains("CLUAIZ SOVEREIGN COMPATIBILITY PATCH") {
            println!("cargo:warning=💉 Patching gguf.cpp for BitNet GGUF Legacy Compatibility (Type 42)...");
            content = content.replace(target_check, patch);
            std::fs::write(&gguf_cpp_path, content).unwrap();
            println!("cargo:warning=✅ GGUF Compatibility Patch Applied!");
        } else {
            println!("cargo:warning=⚠️ GGUF Compatibility Patch already applied.");
        }
    }

    // ── GPU Driver Logic (Sovereign Dispatch) ─────────────────────────
    let feature_cuda     = env::var("CARGO_FEATURE_CUDA").is_ok();
    let feature_metal    = env::var("CARGO_FEATURE_METAL").is_ok();
    let feature_vulkan   = env::var("CARGO_FEATURE_VULKAN").is_ok();
    let feature_rocm     = env::var("CARGO_FEATURE_ROCM").is_ok();
    let feature_openvino = env::var("CARGO_FEATURE_OPENVINO").is_ok();
    let feature_sycl     = env::var("CARGO_FEATURE_SYCL").is_ok();
    let feature_qnn      = env::var("CARGO_FEATURE_QNN").is_ok();
    let feature_cann     = env::var("CARGO_FEATURE_CANN").is_ok();

    // Explicitly disable backends to prevent auto-detection "Bakchodi"
    config.define("GGML_CUDA",     if feature_cuda     { "ON" } else { "OFF" });
    config.define("GGML_METAL",    if feature_metal    { "ON" } else { "OFF" });
    config.define("GGML_VULKAN",   if feature_vulkan   { "ON" } else { "OFF" });
    config.define("GGML_HIPBLAS",  if feature_rocm     { "ON" } else { "OFF" });
    config.define("GGML_OPENVINO", if feature_openvino { "ON" } else { "OFF" });
    config.define("GGML_SYCL",     if feature_sycl     { "ON" } else { "OFF" });
    config.define("GGML_QNN",      if feature_qnn      { "ON" } else { "OFF" });
    config.define("GGML_CANN",     if feature_cann     { "ON" } else { "OFF" });

    if feature_openvino {
        if let Ok(v) = env::var("OpenCL_INCLUDE_DIR") { config.define("OpenCL_INCLUDE_DIR", &v); }
        if let Ok(v) = env::var("OpenCL_LIBRARY")     { 
            config.define("OpenCL_LIBRARY",     &v); 
            if let Some(parent) = Path::new(&v).parent() {
                println!("cargo:rustc-link-search=native={}", parent.display());
            }
        }
        if let Ok(v) = env::var("OpenVINO_DIR")        { config.define("OpenVINO_DIR",       &v); }
        // Hard-link OpenVINO runtime libraries to fix LNK1104: cannot open file 'tbb12.lib'
        if let Ok(v) = env::var("INTEL_OPENVINO_DIR") {
            println!("cargo:rustc-link-search=native={}/runtime/lib/intel64/Release", v);
            println!("cargo:rustc-link-search=native={}/runtime/3rdparty/tbb/lib", v);
        }
    }

    if feature_sycl {
        if let Ok(v) = env::var("DPCPP_CXX") { config.define("CMAKE_CXX_COMPILER", &v); }
        if let Ok(v) = env::var("DPCPP_CC")  { config.define("CMAKE_C_COMPILER",   &v); }
    }

    // Use MSVC-style flags only on Windows.
    if target_os == "windows" {
        config.cxxflag("/EHsc");
    }

    if feature_cann {
        config.define("SOC_TYPE", "ascend910b");
    }

    let dst = config.build();

    // ═══════════════════════════════════════════════════════════════
    // PHASE 3: INDUSTRIAL LINKAGE
    // ═══════════════════════════════════════════════════════════════
    println!("cargo:rustc-link-search=native={}/lib",   dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    find_and_link_search_paths(&dst);

    // ── Core libs (always present) ────────────────────────────────────
    println!("cargo:rustc-link-lib=static=llama");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");

    // ── Windows: explicit backend static libs ─────────────────────────
    // On Windows DLL builds, ALL symbols must resolve at link time.
    // ggml.lib's backend-registry calls backend-specific functions which
    // live in their own libs. Without explicit linking → LNK2019.
    // On Linux/macOS, the dynamic linker resolves these at runtime.
    if target_os == "windows" {
        if feature_cuda   {
            println!("cargo:rustc-link-lib=static=ggml-cuda");
            println!("cargo:rustc-link-arg=/WHOLEARCHIVE:ggml-cuda.lib");
        }
        if feature_vulkan { 
            println!("cargo:rustc-link-lib=static:+whole-archive=ggml-vulkan"); 
            if let Ok(vulkan_sdk) = env::var("VULKAN_SDK") {
                println!("cargo:rustc-link-search=native={}/Lib", vulkan_sdk);
            }
            println!("cargo:rustc-link-lib=dylib=vulkan-1");
        }
        if feature_rocm   { println!("cargo:rustc-link-lib=static:+whole-archive=ggml-hip"); }
        if feature_openvino { 
            println!("cargo:rustc-link-lib=static:+whole-archive=ggml-openvino");
            println!("cargo:rustc-link-lib=dylib=OpenCL"); 
            println!("cargo:rustc-link-lib=dylib=openvino"); // Core OpenVINO Runtime symbols
        }
        if feature_sycl {
            println!("cargo:rustc-link-lib=static:+whole-archive=ggml-sycl");
            println!("cargo:rustc-link-lib=dylib=sycl"); // Intel oneAPI SYCL runtime
            
            // Link Intel oneMKL using the Single Dynamic Library (mkl_rt) for reliable dispatching
            println!("cargo:rustc-link-lib=dylib=mkl_rt");
            
            // Link Intel oneDNN (required for ggml-sycl neural operations)
            println!("cargo:rustc-link-lib=dylib=dnnl");
            
            // Link TBB (dependency for MKL/DNNL)
            println!("cargo:rustc-link-lib=dylib=tbb12");

            // Hard-link search paths for SYCL, MKL, and DNNL on Windows
            let oneapi_root = env::var("ONEAPI_ROOT").unwrap_or_else(|_| "C:\\Program Files (x86)\\Intel\\oneAPI".to_string());
            println!("cargo:rustc-link-search=native={}\\compiler\\latest\\windows\\lib", oneapi_root);
            println!("cargo:rustc-link-search=native={}\\compiler\\latest\\windows\\lib\\intel64", oneapi_root);
            println!("cargo:rustc-link-search=native={}\\mkl\\latest\\lib\\intel64", oneapi_root);
            println!("cargo:rustc-link-search=native={}\\dnnl\\latest\\lib", oneapi_root);
            println!("cargo:rustc-link-search=native={}\\tbb\\latest\\lib\\intel64\\vc14", oneapi_root);
        }
    }

    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=static=ggml-metal");
        println!("cargo:rustc-link-lib=static=ggml-blas");
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }

    // ── OS-specific system libraries ──────────────────────────────────
    if target_os == "windows" {
        println!("cargo:rustc-link-lib=dylib=advapi32");
        println!("cargo:rustc-link-lib=dylib=user32");
        println!("cargo:rustc-link-lib=dylib=ws2_32");

        // FIX 1: CUDA Runtime + cuBLAS import libraries.
        //
        // These are NOT statically embedded inside ggml-cuda.lib.
        // ggml-cuda.lib calls cudaLaunchCooperativeKernel, cudaFuncSetAttribute,
        // __cudaRegisterFatBinary, cublasCreate_v2 etc. which live in:
        //   cudart.lib  → cudart64_*.dll  (CUDA Runtime API)
        //   cublas.lib  → cublas64_*.dll  (cuBLAS)
        //   cublasLt.lib→ cublasLt64_*.dll
        //   cuda.lib    → nvcuda.dll      (CUDA Driver API: cuDeviceGet, cuMemCreate...)
        //
        // jimver/cuda-toolkit GHA action sets CUDA_PATH to the toolkit root.
        // e.g. "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.8"
        // Import libs are at $CUDA_PATH\lib\x64\*.lib
        if feature_cuda {
            if let Ok(cuda_path) = env::var("CUDA_PATH") {
                println!("cargo:rustc-link-search=native={}/lib/x64", cuda_path);
            }
            println!("cargo:rustc-link-lib=dylib=cudart");
            println!("cargo:rustc-link-lib=dylib=cublas");
            println!("cargo:rustc-link-lib=dylib=cublasLt");
            println!("cargo:rustc-link-lib=dylib=cuda");
        }
    } else if target_os == "macos" {
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=MetalKit");
        println!("cargo:rustc-link-lib=framework=Accelerate");
        println!("cargo:rustc-link-lib=dylib=c++");
    } else if target_os == "ios" {
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=UIKit");
        println!("cargo:rustc-link-lib=framework=Accelerate");
        println!("cargo:rustc-link-lib=dylib=c++");
    } else {
        // Linux / Android
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    println!("cargo:warning=🧿 [Llama-Engine] Industrial CMake Build Complete.");
}

fn find_and_link_search_paths(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut has_lib = false;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                find_and_link_search_paths(&path);
            } else if let Some(ext) = path.extension() {
                if ext == "a" || ext == "lib" {
                    has_lib = true;
                }
            }
        }
        if has_lib {
            println!("cargo:rustc-link-search=native={}", dir.display());
        }
    }
}
