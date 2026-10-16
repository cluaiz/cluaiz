use std::env;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR").unwrap();
    let mut llama_path = Path::new(&out_dir).join("llama.cpp");

    // ═══════════════════════════════════════════════════════════════
    // PHASE 1: REPOSITORY INITIALIZATION (Prioritize local cached copy)
    // ═══════════════════════════════════════════════════════════════
    let local_cache_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("llama.cpp");
    if local_cache_path.exists() {
        println!("cargo:warning=🔩 Found local cached llama.cpp at {:?}", local_cache_path);
        // Rebuild when the fork's sources change, not only when this script does
        for dir in ["include", "src", "common", "ggml/include", "ggml/src", "tools/mtmd", "vendor", "CMakeLists.txt"] {
            println!("cargo:rerun-if-changed={}", local_cache_path.join(dir).display());
        }
        llama_path = local_cache_path;
    } else if !llama_path.exists() {
        println!("cargo:warning=🔩 Cloning official cluaiz/llama.cpp engine...");
        let status = Command::new("git")
            .args(["clone", "--depth", "1", "--branch", "cluaiz-main", "https://github.com/cluaiz/llama.cpp", llama_path.to_str().unwrap()])
            .status()
            .expect("Failed to clone llama.cpp");
        if !status.success() { panic!("Clone failed"); }
    }

    generate_mtmd_bindings(&llama_path, Path::new(&out_dir))?;

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
        .define("LLAMA_BUILD_MTMD",     "ON")
        // Shared media owns file decoding. Do not enable mtmd's FFmpeg subprocess path.
        .define("MTMD_VIDEO",           "OFF")
        .define("LLAMA_BUILD_APP",      "OFF")
        .define("LLAMA_BUILD_COMMON",   "ON")
        .define("LLAMA_STATIC",         "ON")
        .define("BUILD_SHARED_LIBS",    "OFF")
        // Use MultiThreadedDLL (/MD) to match Rust's default dynamic CRT on Windows MSVC,
        // and to avoid LNK2038 mismatch with other C++ libraries like ONNX Runtime (ort) which use /MD.
        // CMAKE_POLICY_DEFAULT_CMP0091=NEW is required for CMake 3.14 to respect CMAKE_MSVC_RUNTIME_LIBRARY
        .define("CMAKE_POLICY_DEFAULT_CMP0091", "NEW")
        .define("CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDLL")
        .profile("Release");

    // cmake-rs replaces CMAKE_<LANG>_FLAGS_RELEASE on MSVC and drops every /O flag, so ggml's CPU code
    // was compiled unoptimized (/Od). Give it CMake's own Release flags back (what upstream builds use).
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        for lang in ["C", "CXX"] {
            config.define(format!("CMAKE_{}_FLAGS_RELEASE", lang), "/MD /O2 /Ob2 /DNDEBUG");
        }
    }

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

    // ── GPU Driver Logic ──────────────────────────────────────────────
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
    if feature_cuda {
        let cuda_arch = env::var("CUDA_ARCHITECTURES").unwrap_or_else(|_| "86;89".to_string());
        config.define("CMAKE_CUDA_ARCHITECTURES", cuda_arch);
    }
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

    // Engine-owned ABI adapter; the pinned upstream reasoning implementation is linked below.
    println!("cargo:rerun-if-changed=src/ffi/reasoning_bridge.cpp");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(llama_path.join("include"))
        .include(llama_path.join("ggml/include"))
        .include(llama_path.join("common"))
        .file("src/ffi/reasoning_bridge.cpp")
        .flag_if_supported("/EHsc")
        .compile("cluaiz-reasoning-bridge");

    // ═══════════════════════════════════════════════════════════════
    // PHASE 3: INDUSTRIAL LINKAGE
    // ═══════════════════════════════════════════════════════════════
    println!("cargo:rustc-link-search=native={}/lib",   dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    find_and_link_search_paths(&dst);

    // ── Core libs (always present) ────────────────────────────────────
    println!("cargo:rustc-link-lib=static=mtmd");
    // mtmd's helper object uses vendor hashing; Cargo does not inherit CMake target links.
    println!("cargo:rustc-link-lib=static=vendor-hash");
    println!("cargo:rustc-link-lib=static=llama");
    println!("cargo:rustc-link-lib=static=llama-common");
    println!("cargo:rustc-link-lib=static=llama-common-base");
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
    Ok(())
}

/// Generate only the pinned multimodal C API; opaque llama/ggml handles stay pointers.
fn generate_mtmd_bindings(source: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mtmd = source.join("tools/mtmd");
    let bindings = bindgen::Builder::default()
        .header(mtmd.join("mtmd-helper.h").to_string_lossy())
        .clang_arg(format!("-I{}", source.join("include").display()))
        .clang_arg(format!("-I{}", source.join("ggml/include").display()))
        .allowlist_type("mtmd_context_params|mtmd_input_text|mtmd_input_part")
        .allowlist_function(concat!(
            "mtmd_(context_params_default|default_marker|init_from_file|free|",
            "support_vision|support_audio|get_audio_sample_rate|get_marker|",
            "bitmap_(init|init_from_audio|free|get_nx|get_ny|get_data|get_n_bytes|is_audio|set_id)|",
            "input_chunks_(init|free|size|get)|input_chunk_(get_type|get_tokens_text|get_n_tokens|get_id)|",
            "tokenize|tokenize_from_parts|encode_chunk|get_output_embd|",
            "helper_(get_n_tokens|get_n_pos|eval_chunks|eval_chunk_single|decode_image_chunk))"
        ))
        .opaque_type("llama_model|llama_context|ggml_tensor|ggml_backend_device")
        .generate_comments(false)
        .derive_debug(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()?;
    bindings.write_to_file(output.join("mtmd_bindings.rs"))?;
    Ok(())
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
