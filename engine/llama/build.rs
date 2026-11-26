use std::env;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=llama-revision.txt");

    let out_dir = env::var("OUT_DIR").unwrap();
    let mut llama_path = Path::new(&out_dir).join("llama.cpp");

    // ═══════════════════════════════════════════════════════════════
    // PHASE 1: REPOSITORY INITIALIZATION (Prioritize local cached copy)
    // ═══════════════════════════════════════════════════════════════
    let local_cache_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("llama.cpp");
    let use_local_cache = local_cache_path.join("CMakeLists.txt").is_file();
    if use_local_cache {
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

    if !use_local_cache {
        // Generated fallback checkouts must match the native revision reviewed with this Rust ABI.
        let revision = include_str!("llama-revision.txt").trim();
        let head = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(&llama_path).output()?;
        if !head.status.success() { return Err("Cannot read fallback native revision".into()); }
        if String::from_utf8(head.stdout)?.trim() != revision {
            let dirty = Command::new("git").args(["status", "--porcelain"]).current_dir(&llama_path).output()?;
            if !dirty.status.success() || !dirty.stdout.is_empty() {
                return Err("Fallback native checkout has local changes; preserve them before updating its revision".into());
            }
            for args in [vec!["fetch", "--depth", "1", "origin", revision], vec!["checkout", "--detach", revision]] {
                if !Command::new("git").args(args).current_dir(&llama_path).status()?.success() {
                    return Err("Cannot prepare pinned fallback native checkout".into());
                }
            }
        }
    }

    generate_mtmd_bindings(&llama_path, Path::new(&out_dir))?;

    // ═══════════════════════════════════════════════════════════════
    // PHASE 2: INDUSTRIAL CMAKE BUILD
    // ═══════════════════════════════════════════════════════════════
    let target_os   = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    let mut config = cmake::Config::new(&llama_path);

    // Windows and Linux: every ggml backend is a library the engine loads at start (llama.cpp's
    // GGML_BACKEND_DL), so the core starts without any GPU runtime and a GPU pack is one more file
    // beside it. Apple keeps Metal built in.
    let dynamic_backends = matches!(target_os.as_str(), "windows" | "linux");

    // ── CPU SIMD Vectorization Optimization ──────────────────────────
    let is_ci = env::var("GITHUB_ACTIONS").is_ok();

    if dynamic_backends {
        // One CPU backend per instruction set; ggml loads the best one this processor supports
        config.define("GGML_NATIVE", "OFF");
        let variants = target_arch == "x86_64" || (target_arch == "aarch64" && target_os == "linux");
        config.define("GGML_CPU_ALL_VARIANTS", if variants { "ON" } else { "OFF" });
    } else if is_ci {
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
        // Cluaiz uses its own CLI and Rust/rustls downloader, not llama.cpp's tools or HTTPS client.
        .define("LLAMA_BUILD_TOOLS",    "OFF")
        .define("LLAMA_BUILD_MTMD",     "ON")
        // Shared media owns file decoding. Do not enable mtmd's FFmpeg subprocess path.
        .define("MTMD_VIDEO",           "OFF")
        .define("LLAMA_BUILD_APP",      "OFF")
        .define("LLAMA_BUILD_COMMON",   "ON")
        .define("LLAMA_OPENSSL",        "OFF")
        .define("LLAMA_STATIC",         if dynamic_backends { "OFF" } else { "ON" })
        .define("BUILD_SHARED_LIBS",    if dynamic_backends { "ON" } else { "OFF" })
        .define("GGML_BACKEND_DL",      if dynamic_backends { "ON" } else { "OFF" })
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
    config.define("GGML_HIP",      if feature_rocm     { "ON" } else { "OFF" });
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
    // A GPU pack finds the runtime libraries shipped beside it (cudart, cuBLAS)
    if dynamic_backends && target_os == "linux" {
        config.define("CMAKE_INSTALL_RPATH", "$ORIGIN");
    }

    if feature_cann {
        config.define("SOC_TYPE", "ascend910b");
    }

    let dst = config.build();

    // Windows: ggml-base and the CPU backend modules built again with clang and LLVM OpenMP, as llama.cpp's
    // own release builds them
    let llvm_cpu = if dynamic_backends && target_os == "windows" && target_arch == "x86_64"
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        build_llvm_cpu_backends(&llama_path, Path::new(&out_dir), &dst.join("bin"))
    } else {
        None
    };

    // Engine-owned ABI adapter; the pinned upstream reasoning implementation is linked below.
    println!("cargo:rerun-if-changed=src/ffi/reasoning_bridge.cpp");
    println!("cargo:rerun-if-changed=src/ffi/chat_bridge.cpp");
    println!("cargo:rerun-if-changed=src/ffi/memory_bridge.cpp");
    println!("cargo:rerun-if-changed=src/ffi/speculative_bridge.cpp");
    let mut bridge = cc::Build::new();
    if dynamic_backends {
        // The bridge calls into the llama / ggml libraries, not into copies linked beside it
        bridge.define("LLAMA_SHARED", None).define("GGML_SHARED", None);
    }
    bridge
        .cpp(true)
        .std("c++17")
        .include(llama_path.join("include"))
        .include(llama_path.join("ggml/include"))
        .include(llama_path.join("common"))
        .include(llama_path.join("vendor"))
        .file("src/ffi/reasoning_bridge.cpp")
        .file("src/ffi/chat_bridge.cpp")
        .file("src/ffi/memory_bridge.cpp")
        .file("src/ffi/speculative_bridge.cpp")
        .flag_if_supported("/EHsc")
        .compile("cluaiz-reasoning-bridge");

    // ═══════════════════════════════════════════════════════════════
    // PHASE 3: INDUSTRIAL LINKAGE
    // ═══════════════════════════════════════════════════════════════
    println!("cargo:rustc-link-search=native={}/lib",   dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    find_and_link_search_paths(&dst);

    if dynamic_backends {
        // Only the libraries the engine calls are linked; the CPU and GPU backends are loaded at start
        for lib in ["mtmd", "llama", "llama-common", "ggml", "ggml-base"] {
            println!("cargo:rustc-link-lib=dylib={}", lib);
        }
        println!("cargo:rustc-link-lib=static=llama-common-base");
        // The LLVM-built ggml-base, CPU modules and OpenMP runtime replace the MSVC ones when they built
        let out = Path::new(&out_dir);
        copy_runtime_libraries(&[dst.join("bin"), dst.join("lib")], out, |name| llvm_cpu.is_none() || !is_llvm_built(name))?;
        if let Some(dir) = &llvm_cpu {
            copy_runtime_libraries(&[dir.clone()], out, is_llvm_built)?;
        }
        if target_os == "windows" {
            println!("cargo:rustc-link-lib=dylib=advapi32");
            println!("cargo:rustc-link-lib=dylib=user32");
            println!("cargo:rustc-link-lib=dylib=ws2_32");
        } else {
            println!("cargo:rustc-link-lib=dylib=stdc++");
        }
        println!("cargo:warning=🧿 [Llama-Engine] Industrial CMake Build Complete.");
        return Ok(());
    }

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

    // ── Apple / Android: backends linked in ───────────────────────────
    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=static=ggml-metal");
        println!("cargo:rustc-link-lib=static=ggml-blas");
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }

    // ── OS-specific system libraries ──────────────────────────────────
    if target_os == "macos" {
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
        // Android
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

/// A ggml CPU backend module (`ggml-cpu-haswell.dll`, ...)
fn is_cpu_backend(name: &str) -> bool {
    name.starts_with("ggml-cpu") && name.ends_with(".dll")
}

/// LLVM's OpenMP runtime, which the LLVM-built ggml-base and CPU modules load from their own folder
const LLVM_OPENMP_RUNTIME: &str = "libomp.dll";

/// The libraries the LLVM build provides on Windows (llama.cpp's windows-cpu release job builds the same
/// with clang; its windows-cuda job builds only ggml-cuda, with MSVC)
fn is_llvm_built(name: &str) -> bool {
    is_cpu_backend(name) || name == "ggml-base.dll" || name == LLVM_OPENMP_RUNTIME
}

/// ggml-base and the CPU backend modules built with clang and LLVM OpenMP, as llama.cpp's Windows release
/// builds them (its windows-cpu job with `cmake/x64-windows-llvm.cmake`, used here as is); its CUDA module is
/// built with MSVC, as here. MSVC's CPU modules were slower on the same machine
/// (test/cloud/runs/2026-11-23-run47-laptop-warm-tg-gap: CPU-only pp512 121 vs 137 t/s). Without OpenMP the
/// modules would use ggml's own thread pool, which Cluaiz does not attach to its contexts: 4x slower MoE split
/// generation on the Windows laptop (2026-11-23). Returns the folder holding ggml-base, the modules and `libomp.dll`; None (with a
/// warning, the MSVC libraries stay) when LLVM or Ninja is missing or the build fails, so a build never breaks
/// on it. `CLUAIZ_REQUIRE_LLVM_CPU=1` (release builds) makes that an error.
fn build_llvm_cpu_backends(llama_path: &Path, out_dir: &Path, msvc_bin: &Path) -> Option<std::path::PathBuf> {
    println!("cargo:rerun-if-env-changed=CLUAIZ_LLVM_DIR");
    println!("cargo:rerun-if-env-changed=CLUAIZ_REQUIRE_LLVM_CPU");
    let required = env::var("CLUAIZ_REQUIRE_LLVM_CPU").as_deref() == Ok("1");
    let skip = |why: String| {
        if required {
            panic!("CLUAIZ_REQUIRE_LLVM_CPU=1 but the LLVM CPU backends were not built: {}", why);
        }
        println!("cargo:warning=CPU backends built with MSVC, slower than llama.cpp's release: {}", why);
        None
    };
    // LLVM's bin folder: CLUAIZ_LLVM_DIR, else clang on PATH, else LLVM's default install folder
    let has_clang = |dir: &Path| dir.join("clang.exe").is_file() && dir.join("clang++.exe").is_file();
    let llvm_bin = match env::var_os("CLUAIZ_LLVM_DIR") {
        Some(dir) => Some(std::path::PathBuf::from(dir)).filter(|dir| has_clang(dir)),
        None => env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .chain([std::path::PathBuf::from(r"C:\Program Files\LLVM\bin")])
            .find(|dir| has_clang(dir)),
    };
    let Some(llvm_bin) = llvm_bin else {
        return skip("clang not found (CLUAIZ_LLVM_DIR names LLVM's bin folder)".to_string());
    };
    // LLVM's own OpenMP: CMake's search would otherwise take the libomp.lib of Visual Studio, whose
    // libomp140.x86_64.dll is not a runtime to ship
    let openmp_dll = llvm_bin.join(LLVM_OPENMP_RUNTIME);
    let openmp_lib = llvm_bin.parent().map(|root| root.join("lib").join("libomp.lib")).unwrap_or_default();
    if !openmp_dll.is_file() || !openmp_lib.is_file() {
        return skip(format!("LLVM OpenMP not found ({} / {})", openmp_dll.display(), openmp_lib.display()));
    }
    if Command::new("ninja").arg("--version").output().is_err() {
        return skip("ninja not found".to_string());
    }
    // cmake-rs gives the build MSVC's environment (INCLUDE, LIB, PATH: clang's msvc target links with it);
    // LLVM goes first on PATH, where the toolchain file looks for clang
    let msvc = cc::Build::new().get_compiler();
    let mut path = std::ffi::OsString::from(llvm_bin.as_os_str());
    let inherited = msvc.env().iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")).map(|(_, v)| v.clone());
    for part in [inherited, env::var_os("PATH")].into_iter().flatten() {
        path.push(";");
        path.push(part);
    }
    // ggml-base and the same CPU variants as the MSVC build (its module names are the CMake targets). Not all
    // of upstream's clang list (that adds ivybridge, piledriver, cooperlake, zen4, sapphirerapids): clang 22
    // did not finish the sapphirerapids variant's amx/mmq.cpp in 30 minutes (upstream's CI uses clang 20)
    let mut targets: Vec<String> = std::fs::read_dir(msvc_bin)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|e| e.file_name().to_str().filter(|n| is_cpu_backend(n)).map(|n| n.trim_end_matches(".dll").to_string()))
                .collect()
        })
        .unwrap_or_default();
    if targets.is_empty() {
        return skip(format!("no MSVC CPU module in {} to match", msvc_bin.display()));
    }
    targets.sort();
    targets.insert(0, "ggml-base".to_string());
    let (first, rest) = targets.split_first().expect("ggml-base is in the list");
    let dst = out_dir.join("cpu-llvm");
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut config = cmake::Config::new(llama_path);
        config
            .out_dir(&dst)
            .generator("Ninja")
            .no_default_flags(true)
            .profile("Release")
            .define("CMAKE_TOOLCHAIN_FILE", llama_path.join("cmake/x64-windows-llvm.cmake"))
            // cmake-rs would otherwise replace CMake's Release flags (see the MSVC build above)
            .define("CMAKE_C_FLAGS_RELEASE", "-O3 -DNDEBUG")
            .define("CMAKE_CXX_FLAGS_RELEASE", "-O3 -DNDEBUG")
            // clang 22 declares _mm_prefetch as a function taking `const char *` and stops on ggml's block
            // pointers (arch/x86/quants.c); llama.cpp's release uses clang 20, where it is a warning
            .cflag("-Wno-error=incompatible-pointer-types")
            // the same C runtime as the MSVC libraries and Rust (/MD)
            .define("CMAKE_POLICY_DEFAULT_CMP0091", "NEW")
            .define("CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDLL")
            .define("BUILD_SHARED_LIBS", "ON")
            .define("GGML_BACKEND_DL", "ON")
            .define("GGML_NATIVE", "OFF")
            .define("GGML_CPU_ALL_VARIANTS", "ON")
            .define("GGML_OPENMP", "ON")
            .define("OpenMP_libomp_LIBRARY", &openmp_lib)
            .define("GGML_CUDA", "OFF")
            .define("GGML_VULKAN", "OFF")
            .define("LLAMA_BUILD_COMMON", "OFF")
            .define("LLAMA_BUILD_MTMD", "OFF")
            .define("LLAMA_BUILD_TOOLS", "OFF")
            .define("LLAMA_BUILD_TESTS", "OFF")
            .define("LLAMA_BUILD_EXAMPLES", "OFF")
            .define("LLAMA_BUILD_SERVER", "OFF")
            .define("LLAMA_BUILD_APP", "OFF")
            .define("LLAMA_OPENSSL", "OFF")
            .env("PATH", &path)
            .build_target(first);
        // the other CPU modules: more targets for Ninja
        for target in rest {
            config.build_arg(target);
        }
        config.build()
    }));
    let bin = match built {
        Ok(root) => root.join("build").join("bin"),
        Err(_) => return skip("the LLVM CMake build failed (its output is above)".to_string()),
    };
    let modules = std::fs::read_dir(&bin).map(|entries| {
        entries.filter_map(Result::ok).filter(|e| is_cpu_backend(&e.file_name().to_string_lossy())).count()
    });
    if modules.unwrap_or(0) == 0 || !bin.join("ggml-base.dll").is_file() {
        return skip(format!("no ggml-base or CPU module in {}", bin.display()));
    }
    if let Err(e) = std::fs::copy(&openmp_dll, bin.join(LLVM_OPENMP_RUNTIME)) {
        return skip(format!("copying {}: {}", openmp_dll.display(), e));
    }
    Some(bin)
}

/// Puts the llama / ggml libraries and backend modules of `dirs` that `keep` accepts beside the binaries
/// of this profile (`target/<profile>` and its `deps` for tests), where the loader and ggml look for them.
fn copy_runtime_libraries(
    dirs: &[std::path::PathBuf],
    out_dir: &Path,
    keep: impl Fn(&str) -> bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // OUT_DIR is target/<profile>/build/<crate>-<hash>/out, or target/<profile>/build/<crate>/<hash>/out
    // since Rust 1.100: the profile is the parent of the nearest `build` folder
    let profile = out_dir
        .ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "build"))
        .and_then(Path::parent)
        .ok_or("OUT_DIR is not inside a target profile")?;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let text = name.to_string_lossy();
            if !(text.ends_with(".dll") || text.ends_with(".so") || text.contains(".so.")) || !keep(&text) {
                continue;
            }
            // `deps` holds the test binaries in the old layout only; Rust 1.100 has no such folder
            for to in [profile.to_path_buf(), profile.join("deps")].into_iter().filter(|to| to.is_dir()) {
                let target = to.join(&name);
                if entry.file_type()?.is_symlink() {
                    // libllama.so -> libllama.so.0: keep the link, not a second copy
                    #[cfg(unix)]
                    {
                        let _ = std::fs::remove_file(&target);
                        std::os::unix::fs::symlink(std::fs::read_link(entry.path())?, &target)?;
                    }
                    continue;
                }
                let source = entry.metadata()?;
                let same = std::fs::metadata(&target)
                    .map(|t| t.len() == source.len() && t.modified().ok() >= source.modified().ok())
                    .unwrap_or(false);
                if !same {
                    std::fs::copy(entry.path(), &target)?;
                }
            }
        }
    }
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
