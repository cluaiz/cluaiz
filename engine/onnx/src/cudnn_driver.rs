//! Setup-time provisioning and non-blocking runtime readiness verification for NVIDIA CUDA
//! (`cudart`, `cublas`, `cublasLt`, `cufft`) and cuDNN 9 shared libraries required by the
//! ONNX Runtime CUDA Execution Provider (`onnxruntime_providers_cuda`).
//!
//! Provisioning runs during setup/bootstrap (`provision_cuda_runtime_at_setup`) so user inference
//! requests never block waiting for multi-hundred-megabyte redistributable downloads. At model
//! load time, `is_cuda_runtime_ready_or_spawn_background` performs a fast synchronous file check:
//! if all required DLLs/SOs are present, it registers `<cluaiz_home>/engine/drivers` on `PATH` and
//! returns `true`; if any are missing, it spawns a single background provisioning thread and
//! immediately returns `false` so the current request proceeds on CPU/DirectML without delay.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const CUDNN_SOURCES_JSON: &str = include_str!("data/cudnn_sources.json");

#[derive(Debug, Clone, Deserialize)]
pub struct CudnnManifest {
    pub version: String,
    pub cuda_variant: String,
    pub base_url: String,
    pub platforms: Vec<CudnnPlatformSource>,
    #[serde(default)]
    pub cuda_variants: Vec<CudaVariantManifest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CudnnPlatformSource {
    pub platform: String,
    pub relative_path: String,
    pub bytes: u64,
    pub sha256: String,
    pub sentinel_library: String,
    pub library_prefix: String,
    pub library_extension: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CudaVariantManifest {
    pub cuda_variant: String,
    pub cuda_version: String,
    pub cudnn_version: String,
    pub components: Vec<CudaComponentManifest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CudaComponentManifest {
    pub component: String,
    pub version: String,
    pub base_url: String,
    pub platforms: Vec<CudaComponentPlatformSource>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CudaComponentPlatformSource {
    pub platform: String,
    pub relative_path: String,
    pub bytes: u64,
    pub sha256: String,
    pub sentinel_libraries: Vec<String>,
    pub library_prefix: String,
    pub library_extension: String,
}

/// Returns the parsed `data/cudnn_sources.json` manifest.
pub fn manifest() -> Option<&'static CudnnManifest> {
    static MANIFEST: OnceLock<Option<CudnnManifest>> = OnceLock::new();
    MANIFEST
        .get_or_init(|| match serde_json::from_str(CUDNN_SOURCES_JSON) {
            Ok(parsed) => Some(parsed),
            Err(error) => {
                tracing::error!(%error, "cudnn_sources.json failed to parse");
                None
            }
        })
        .as_ref()
}

/// Maps the current compilation target to a platform entry in `cudnn_sources.json`.
pub fn current_platform_key() -> Option<&'static str> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Some("windows-x86_64");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Some("linux-x86_64");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Some("linux-aarch64");
    #[allow(unreachable_code)]
    None
}

/// Resolves the legacy single-archive `CudnnPlatformSource` for a given platform key.
pub fn platform_source(platform_key: &str) -> Option<&'static CudnnPlatformSource> {
    manifest()?.platforms.iter().find(|entry| entry.platform == platform_key)
}

/// Resolves the `CudaVariantManifest` for `"cuda12"` or `"cuda13"`.
pub fn variant_manifest(cuda_variant: &str) -> Option<&'static CudaVariantManifest> {
    manifest()?.cuda_variants.iter().find(|v| v.cuda_variant == cuda_variant)
}

/// Inspects raw binary bytes of `onnxruntime_providers_cuda.dll` / `libonnxruntime_providers_cuda.so`
/// to determine whether the provider was compiled against CUDA 13 or CUDA 12.
pub fn detect_cuda_variant_from_provider_bytes(bytes: &[u8]) -> Option<&'static str> {
    const CUDA13: &[&[u8]] = &[b"cublas64_13.dll", b"cudart64_13.dll", b"libcublas.so.13", b"libcudart.so.13"];
    const CUDA12: &[&[u8]] = &[b"cublas64_12.dll", b"cudart64_12.dll", b"libcublas.so.12", b"libcudart.so.12"];
    if CUDA13.iter().any(|m| bytes.windows(m.len()).any(|w| w.eq_ignore_ascii_case(m))) {
        return Some("cuda13");
    }
    if CUDA12.iter().any(|m| bytes.windows(m.len()).any(|w| w.eq_ignore_ascii_case(m))) {
        return Some("cuda12");
    }
    None
}

/// Returns candidate directories where the active `onnxruntime_providers_cuda` binary may reside,
/// ordered from most authoritative (current executable dir and `ort.pyke.io` cache) to `drivers_dir`.
fn provider_search_dirs(drivers_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let dfbin = PathBuf::from(local_app_data).join("ort.pyke.io").join("dfbin");
        for target_entry in std::fs::read_dir(&dfbin).into_iter().flatten().flatten() {
            for hash_entry in std::fs::read_dir(target_entry.path()).into_iter().flatten().flatten() {
                dirs.push(hash_entry.path());
            }
        }
    }
    dirs.push(drivers_dir.to_path_buf());
    dirs
}

/// The ONNX Runtime CUDA provider libraries (the shared bridge and the CUDA provider itself).
const PROVIDER_FILES: [&str; 2] = if cfg!(windows) {
    ["onnxruntime_providers_shared.dll", "onnxruntime_providers_cuda.dll"]
} else {
    ["libonnxruntime_providers_shared.so", "libonnxruntime_providers_cuda.so"]
};

/// Synchronizes `onnxruntime_providers_shared` and `onnxruntime_providers_cuda` from the build/cache
/// directory into `drivers_dir` when present, avoiding stale ABI mismatches.
pub fn sync_ort_provider_binaries_to_drivers_dir(drivers_dir: &Path) {
    let search_dirs = provider_search_dirs(drivers_dir);
    for file_name in &PROVIDER_FILES {
        let dest = drivers_dir.join(file_name);
        for dir in &search_dirs {
            if dir == drivers_dir {
                continue;
            }
            let candidate = dir.join(file_name);
            if let Ok(meta) = std::fs::metadata(&candidate) {
                if meta.is_file() && meta.len() > 0 {
                    let needs_copy = std::fs::metadata(&dest).map(|d| d.len() != meta.len()).unwrap_or(true);
                    if needs_copy {
                        let _ = std::fs::create_dir_all(drivers_dir);
                        let _ = std::fs::copy(&candidate, &dest);
                    }
                    break;
                }
            }
        }
    }
}

/// Detects whether the installed `onnxruntime_providers_cuda` binary targets `"cuda12"` or `"cuda13"`.
/// Defaults to `"cuda12"` when no provider binary is found or when no import marker matches.
pub fn detect_provider_cuda_variant(drivers_dir: &Path) -> &'static str {
    for dir in provider_search_dirs(drivers_dir) {
        for name in ["onnxruntime_providers_cuda.dll", "libonnxruntime_providers_cuda.so"] {
            if let Some(variant) = provider_variant(&dir.join(name), drivers_dir) {
                return variant;
            }
        }
    }
    "cuda12"
}

/// File in `drivers_dir` remembering the last provider binary's variant with its size, time and path
const VARIANT_RECORD: &str = "cuda_provider_variant.txt";

/// The CUDA variant of one provider binary. The binary is ~260 MB, so the answer is kept (in memory
/// and in `VARIANT_RECORD`) by its size, modification time and path, and the file is read again
/// only when it changes, not on every command and model load.
fn provider_variant(path: &Path, drivers_dir: &Path) -> Option<&'static str> {
    static SEEN: Mutex<Vec<(String, &'static str)>> = Mutex::new(Vec::new());
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let stamp = format!("{}\t{}\t{}", meta.len(), modified, path.display());
    let known = |variant: &str| ["cuda12", "cuda13"].into_iter().find(|v| *v == variant);

    if let Some(variant) = SEEN.lock().ok()?.iter().find(|(s, _)| *s == stamp).map(|(_, v)| *v) {
        return Some(variant);
    }
    let record = drivers_dir.join(VARIANT_RECORD);
    let recorded = std::fs::read_to_string(&record).ok().and_then(|text| {
        let (variant, rest) = text.trim_end().split_once('\t')?;
        (rest == stamp).then(|| known(variant)).flatten()
    });
    let variant = match recorded {
        Some(variant) => variant,
        None => {
            let variant = detect_cuda_variant_from_provider_bytes(&std::fs::read(path).ok()?)?;
            if drivers_dir.is_dir() {
                let _ = std::fs::write(&record, format!("{variant}\t{stamp}"));
            }
            variant
        }
    };
    if let Ok(mut seen) = SEEN.lock() {
        seen.push((stamp, variant));
    }
    Some(variant)
}

/// A download cut off when its process ended leaves a `.part` file behind; one untouched for an
/// hour is not in progress (downloads time out after 30 minutes), so it is removed.
fn remove_stale_downloads(drivers_dir: &Path) {
    for entry in std::fs::read_dir(drivers_dir).into_iter().flatten().flatten() {
        if !entry.file_name().to_string_lossy().contains(".part") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > std::time::Duration::from_secs(3600));
        if stale && entry.path().is_file() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Default `<cluaiz_home>/engine/drivers` directory.
pub fn default_drivers_dir() -> PathBuf {
    cluaiz_core::EnvironmentManager::current().engine_dir().join("drivers")
}

/// Checks whether a specific sentinel library file exists and is non-empty inside `drivers_dir`.
pub fn is_sentinel_in_drivers_dir(drivers_dir: &Path, sentinel_library: &str) -> bool {
    std::fs::metadata(drivers_dir.join(sentinel_library))
        .map(|meta| meta.is_file() && meta.len() > 0)
        .unwrap_or(false)
}

/// Checks whether the platform's cuDNN 9 sentinel library exists and is non-empty inside `drivers_dir`.
pub fn is_cudnn_in_drivers_dir(drivers_dir: &Path, source: &CudnnPlatformSource) -> bool {
    is_sentinel_in_drivers_dir(drivers_dir, &source.sentinel_library)
}

/// Checks whether all sentinel libraries for a CUDA runtime component are present in `drivers_dir`.
pub fn is_component_in_drivers_dir(drivers_dir: &Path, source: &CudaComponentPlatformSource) -> bool {
    !source.sentinel_libraries.is_empty()
        && source.sentinel_libraries.iter().all(|s| is_sentinel_in_drivers_dir(drivers_dir, s))
}

/// Checks whether all sentinel libraries for a CUDA runtime component are available either in
/// `drivers_dir` or on the operating system's dynamic library search path.
pub fn is_component_available(drivers_dir: &Path, source: &CudaComponentPlatformSource) -> bool {
    !source.sentinel_libraries.is_empty()
        && source.sentinel_libraries.iter().all(|sentinel| {
            is_sentinel_in_drivers_dir(drivers_dir, sentinel) || is_sentinel_on_system_path(sentinel)
        })
}

/// Checks whether the full CUDA + cuDNN 9 runtime suite (`cuda_cudart`, `libcublas`, `libcufft`,
/// `cudnn`) for the detected CUDA variant is available in `drivers_dir` or on the system path.
pub fn is_cuda_runtime_available_in(drivers_dir: &Path) -> bool {
    let Some(key) = current_platform_key() else {
        return false;
    };
    let cuda_variant = detect_provider_cuda_variant(drivers_dir);
    let Some(v_manifest) = variant_manifest(cuda_variant) else {
        return false;
    };
    v_manifest.components.iter().all(|comp| {
        comp.platforms
            .iter()
            .find(|p| p.platform == key)
            .map(|plat_src| is_component_available(drivers_dir, plat_src))
            .unwrap_or(false)
    })
}

/// Checks whether the platform's cuDNN 9 sentinel library is available either in `drivers_dir` or
/// in the operating system's dynamic library search paths.
pub fn is_cudnn_available_in(drivers_dir: &Path) -> bool {
    is_cuda_runtime_available_in(drivers_dir)
}

fn is_sentinel_on_system_path(sentinel_library: &str) -> bool {
    #[cfg(windows)]
    {
        std::env::var_os("PATH")
            .map(|path_os| {
                std::env::split_paths(&path_os).any(|dir| {
                    std::fs::metadata(dir.join(sentinel_library))
                        .map(|m| m.is_file() && m.len() > 0)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        if let Some(ld_os) = std::env::var_os("LD_LIBRARY_PATH") {
            if std::env::split_paths(&ld_os).any(|dir| {
                std::fs::metadata(dir.join(sentinel_library))
                    .map(|m| m.is_file() && m.len() > 0)
                    .unwrap_or(false)
            }) {
                return true;
            }
        }
        ["/usr/local/cuda/lib64", "/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/usr/lib64"]
            .iter()
            .any(|std_dir| {
                std::fs::metadata(Path::new(std_dir).join(sentinel_library))
                    .map(|m| m.is_file() && m.len() > 0)
                    .unwrap_or(false)
            })
    }
}

/// Ensures `drivers_dir` is registered on `PATH` (Windows) so `LoadLibrary` inside
/// `onnxruntime_providers_cuda` can resolve `cudart`, `cublas`, `cublasLt`, `cufft`, and `cudnn`,
/// and that the provider libraries sit beside the executable (see `place_providers_beside_executable`).
pub fn register_drivers_dir_in_path(drivers_dir: &Path) {
    place_providers_beside_executable(drivers_dir);
    #[cfg(windows)]
    {
        if !drivers_dir.exists() {
            return;
        }
        let current_path = std::env::var_os("PATH").unwrap_or_default();
        if !std::env::split_paths(&current_path).any(|p| p == drivers_dir) {
            let mut new_path = drivers_dir.as_os_str().to_os_string();
            if !current_path.is_empty() {
                new_path.push(";");
                new_path.push(current_path);
            }
            std::env::set_var("PATH", new_path);
        }
    }
    #[cfg(not(windows))]
    let _ = drivers_dir;
}

/// ONNX Runtime loads its CUDA provider only from the folder of the module it is built into (the
/// `cluaiz` executable), never from `PATH`. An installed engine keeps the providers in
/// `drivers_dir` and its executable alone in `bin/`, so CUDA was skipped without an error and ONNX
/// models ran on DirectML. A missing provider is linked (or, across drives, copied) beside the
/// executable; one already there (a development build's own) is left as it is.
pub fn place_providers_beside_executable(drivers_dir: &Path) {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return;
    };
    if exe_dir == drivers_dir {
        return;
    }
    for name in PROVIDER_FILES {
        let source = drivers_dir.join(name);
        let target = exe_dir.join(name);
        if target.exists() || !source.is_file() {
            continue;
        }
        if std::fs::hard_link(&source, &target).is_ok() {
            continue;
        }
        // A copy is written aside and renamed, so a cut-off copy is never taken for the provider
        let part = exe_dir.join(format!("{name}.part{}", std::process::id()));
        let copied = std::fs::copy(&source, &part).and_then(|_| std::fs::rename(&part, &target));
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&part);
            tracing::warn!(%error, ?target, "Could not place the ONNX CUDA provider beside the executable; ONNX models will not use CUDA");
        }
    }
}

/// Synchronous setup-time provisioning entry point invoked during `Bootstrapper::sync_neural_stack`,
/// `Calibrate`, and `DevSync`. Downloads and verifies all missing CUDA + cuDNN 9 runtime archives
/// when an NVIDIA GPU is present.
pub fn provision_cuda_runtime_at_setup() -> Result<bool> {
    let hw = cluaiz_hw::HardwareProber::current();
    let has_nvidia = hw.gpus.iter().any(|g| g.vendor.to_uppercase().contains("NVIDIA"));
    let drivers_dir = default_drivers_dir();
    let ready = ensure_cuda_runtime_ready(&drivers_dir, has_nvidia)?;
    if ready {
        register_drivers_dir_in_path(&drivers_dir);
    }
    Ok(ready)
}

/// Alias preserved for callers expecting `ensure_cudnn_ready_default`.
pub fn ensure_cudnn_ready_default() -> Result<bool> {
    provision_cuda_runtime_at_setup()
}

/// Non-blocking model-load readiness check.
///
/// - If all required CUDA + cuDNN 9 runtime libraries are already present on disk, registers
///   `drivers_dir` on `PATH` and immediately returns `true`.
/// - If any library is missing, never blocks the calling model load or inference request: spawns a
///   single background provisioning thread (if an NVIDIA GPU is present) and immediately returns
///   `false` so the caller falls back to CPU/DirectML without delay.
pub fn is_cuda_runtime_ready_or_spawn_background() -> bool {
    static BACKGROUND_PROVISION_SPAWNED: AtomicBool = AtomicBool::new(false);

    let drivers_dir = default_drivers_dir();
    if is_cuda_runtime_available_in(&drivers_dir) {
        register_drivers_dir_in_path(&drivers_dir);
        return true;
    }

    let hw = cluaiz_hw::HardwareProber::current();
    let has_nvidia = hw.gpus.iter().any(|g| g.vendor.to_uppercase().contains("NVIDIA"));
    if !has_nvidia {
        return false;
    }

    if BACKGROUND_PROVISION_SPAWNED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        tracing::info!(
            drivers_dir = ?drivers_dir,
            "CUDA/cuDNN 9 runtime libraries not yet present; spawning background provisioning thread and using CPU/DirectML for the current session"
        );
        let _ = std::thread::Builder::new()
            .name("cluaiz-cuda-driver-bg".to_string())
            .spawn(move || {
                if let Err(error) = provision_cuda_runtime_at_setup() {
                    tracing::warn!(%error, "Background CUDA/cuDNN 9 runtime provisioning did not complete");
                }
            });
    }

    false
}

/// Provisions all missing CUDA + cuDNN 9 runtime components (`cuda_cudart`, `libcublas`,
/// `libcufft`, `cudnn`) for the detected CUDA variant (`cuda12` or `cuda13`) into `drivers_dir`.
pub fn ensure_cuda_runtime_ready(drivers_dir: &Path, has_nvidia_gpu: bool) -> Result<bool> {
    if !has_nvidia_gpu {
        return Ok(false);
    }
    let Some(key) = current_platform_key() else {
        return Ok(false);
    };
    sync_ort_provider_binaries_to_drivers_dir(drivers_dir);
    let cuda_variant = detect_provider_cuda_variant(drivers_dir);
    let Some(v_manifest) = variant_manifest(cuda_variant) else {
        bail!("CUDA variant manifest '{}' is unavailable", cuda_variant);
    };

    if is_cuda_runtime_available_in(drivers_dir) {
        return Ok(true);
    }

    if std::env::var("CLUAIZ_DISABLE_CUDNN_AUTODOWNLOAD")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        tracing::info!("Skipping automatic CUDA/cuDNN 9 download because CLUAIZ_DISABLE_CUDNN_AUTODOWNLOAD is set");
        return Ok(false);
    }

    std::fs::create_dir_all(drivers_dir)
        .with_context(|| format!("failed to create drivers directory {:?}", drivers_dir))?;
    remove_stale_downloads(drivers_dir);

    for comp in &v_manifest.components {
        let Some(source) = comp.platforms.iter().find(|p| p.platform == key) else {
            bail!("no platform entry '{}' for CUDA component '{}' ({})", key, comp.component, cuda_variant);
        };
        if is_component_available(drivers_dir, source) {
            continue;
        }

        let url = format!("{}{}", comp.base_url, source.relative_path);
        let archive_ext = if source.relative_path.ends_with(".zip") { "zip" } else { "tar.xz" };
        let part_archive = drivers_dir.join(format!(
            "{}-{}-{}.part.{}",
            comp.component,
            comp.version,
            std::process::id(),
            archive_ext
        ));

        let mb = source.bytes as f64 / (1024.0 * 1024.0);
        eprintln!(
            "  ⬇️  [Cluaiz CUDA Setup] Downloading NVIDIA {} v{} ({}, {:.1} MB)...",
            comp.component, comp.version, cuda_variant, mb
        );
        tracing::info!(
            component = %comp.component,
            version = %comp.version,
            cuda_variant = %cuda_variant,
            platform = %source.platform,
            bytes = source.bytes,
            "Downloading NVIDIA runtime archive for ONNX CUDA Execution Provider"
        );

        let download_result = (|| -> Result<()> {
            download_to_file(&url, &part_archive, source.bytes)?;
            verify_archive_file(&part_archive, source.bytes, &source.sha256)?;
            if archive_ext == "zip" {
                extract_component_from_zip(&part_archive, drivers_dir, source)?;
            } else {
                extract_component_from_tar_xz(&part_archive, drivers_dir, source)?;
            }
            Ok(())
        })();

        let _ = std::fs::remove_file(&part_archive);
        download_result?;

        if !is_component_in_drivers_dir(drivers_dir, source) {
            bail!(
                "component '{}' extracted but sentinel libraries {:?} were not found in {:?}",
                comp.component,
                source.sentinel_libraries,
                drivers_dir
            );
        }
        eprintln!(
            "  ✅  [Cluaiz CUDA Setup] Installed {} v{} into {:?}",
            comp.component, comp.version, drivers_dir
        );
    }

    let _ = std::fs::write(drivers_dir.join("cudnn9.ready"), &v_manifest.cudnn_version);
    let _ = std::fs::write(
        drivers_dir.join("cuda_runtime.ready"),
        format!("{}:{}", v_manifest.cuda_variant, v_manifest.cuda_version),
    );
    tracing::info!(
        cuda_variant = %v_manifest.cuda_variant,
        cuda_version = %v_manifest.cuda_version,
        cudnn_version = %v_manifest.cudnn_version,
        drivers_dir = ?drivers_dir,
        "NVIDIA CUDA and cuDNN 9 runtime libraries installed into engine drivers directory"
    );
    Ok(true)
}

/// Alias preserved for callers and tests invoking `ensure_cudnn_ready`.
pub fn ensure_cudnn_ready(drivers_dir: &Path, has_nvidia_gpu: bool) -> Result<bool> {
    ensure_cuda_runtime_ready(drivers_dir, has_nvidia_gpu)
}

fn download_to_file(url: &str, dest: &Path, expected_bytes: u64) -> Result<()> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(1800))
        .user_agent("Cluaiz-Engine")
        .build()?
        .get(url)
        .send()
        .with_context(|| format!("GET {url} failed"))?
        .error_for_status()?;

    let file = File::create(dest).with_context(|| format!("failed to create temporary archive {:?}", dest))?;
    let mut writer = BufWriter::new(file);
    let mut reader = response.take(expected_bytes + 1);
    let copied = std::io::copy(&mut reader, &mut writer)
        .with_context(|| format!("failed while streaming download from {url}"))?;
    writer.flush()?;
    if copied != expected_bytes {
        bail!("downloaded size mismatch for {url}: expected {expected_bytes} bytes, got {copied} bytes");
    }
    Ok(())
}

/// Verifies that an archive on disk matches both the pinned byte count and SHA-256 digest.
pub fn verify_archive_file(path: &Path, expected_bytes: u64, expected_sha256: &str) -> Result<()> {
    let meta = std::fs::metadata(path).with_context(|| format!("failed to stat archive {:?}", path))?;
    if meta.len() != expected_bytes {
        bail!("archive byte size mismatch: expected {expected_bytes}, got {}", meta.len());
    }
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual_sha = format!("{:x}", hasher.finalize());
    if !actual_sha.eq_ignore_ascii_case(expected_sha256) {
        bail!("SHA-256 mismatch for {:?}: expected {}, got {}", path, expected_sha256, actual_sha);
    }
    Ok(())
}

/// Extracts matching runtime shared libraries from a verified ZIP archive into `drivers_dir`.
pub fn extract_component_from_zip(
    archive_path: &Path,
    drivers_dir: &Path,
    source: &CudaComponentPlatformSource,
) -> Result<usize> {
    extract_libraries_from_zip(archive_path, drivers_dir, &source.library_prefix, &source.library_extension)
}

/// Extracts matching cuDNN runtime shared libraries from a verified ZIP archive into `drivers_dir`.
pub fn extract_cudnn_from_zip(
    archive_path: &Path,
    drivers_dir: &Path,
    source: &CudnnPlatformSource,
) -> Result<usize> {
    extract_libraries_from_zip(archive_path, drivers_dir, &source.library_prefix, &source.library_extension)
}

fn extract_libraries_from_zip(
    archive_path: &Path,
    drivers_dir: &Path,
    library_prefix: &str,
    library_extension: &str,
) -> Result<usize> {
    let file = File::open(archive_path)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))
        .with_context(|| format!("failed to open ZIP archive {:?}", archive_path))?;
    std::fs::create_dir_all(drivers_dir)?;

    let mut extracted_count = 0usize;
    for idx in 0..zip.len() {
        let mut entry = zip.by_index(idx)?;
        if !entry.is_file() {
            continue;
        }
        let Some(enclosed) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let Some(file_name) = enclosed.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let in_bin_or_lib = enclosed.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s.eq_ignore_ascii_case("bin") || s.eq_ignore_ascii_case("lib")
        });
        if !in_bin_or_lib || !file_name.starts_with(library_prefix) || !file_name.contains(library_extension) {
            continue;
        }

        let target_path = drivers_dir.join(file_name);
        let part_path = drivers_dir.join(format!("{file_name}.part{}", std::process::id()));
        {
            let mut out = BufWriter::new(File::create(&part_path)?);
            std::io::copy(&mut entry, &mut out)?;
            out.flush()?;
        }
        if let Err(err) = std::fs::rename(&part_path, &target_path) {
            let _ = std::fs::remove_file(&part_path);
            return Err(err).with_context(|| format!("failed to place {:?}", target_path));
        }
        extracted_count += 1;
    }

    if extracted_count == 0 {
        bail!("no runtime libraries matching prefix '{}' found inside {:?}", library_prefix, archive_path);
    }
    Ok(extracted_count)
}

fn extract_component_from_tar_xz(
    archive_path: &Path,
    drivers_dir: &Path,
    source: &CudaComponentPlatformSource,
) -> Result<usize> {
    let temp_extract = drivers_dir.join(format!("cuda-extract-{}", std::process::id()));
    std::fs::create_dir_all(&temp_extract)?;

    let status = std::process::Command::new("tar")
        .arg("-xJf")
        .arg(archive_path)
        .arg("-C")
        .arg(&temp_extract)
        .status()
        .context("failed to invoke system 'tar' to extract CUDA/cuDNN .tar.xz archive")?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&temp_extract);
        bail!("tar exited with non-zero status: {status}");
    }

    let mut extracted_count = 0usize;
    let mut stack = vec![temp_extract.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !file_name.starts_with(&source.library_prefix) || !file_name.contains(&source.library_extension) {
                continue;
            }
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.is_file() && meta.len() > 0 {
                    let target = drivers_dir.join(file_name);
                    std::fs::copy(&path, &target)
                        .with_context(|| format!("failed to copy {:?} to {:?}", path, target))?;
                    extracted_count += 1;
                }
            }
        }
    }

    let _ = std::fs::remove_dir_all(&temp_extract);
    if extracted_count == 0 {
        bail!("no shared libraries matching prefix '{}' found in {:?}", source.library_prefix, archive_path);
    }
    Ok(extracted_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_both_cuda12_and_cuda13_with_all_four_components() {
        let m = manifest().expect("cudnn_sources.json must parse");
        assert_eq!(m.version, "9.8.0.87");
        assert_eq!(m.cuda_variants.len(), 2);

        for variant_key in ["cuda12", "cuda13"] {
            let vm = variant_manifest(variant_key).unwrap_or_else(|| panic!("missing variant {variant_key}"));
            let comp_names: Vec<&str> = vm.components.iter().map(|c| c.component.as_str()).collect();
            assert_eq!(comp_names, vec!["cuda_cudart", "libcublas", "libcufft", "cudnn"]);
            for comp in &vm.components {
                assert!(comp.base_url.starts_with("https://developer.download.nvidia.com/"));
                for plat in &comp.platforms {
                    assert_eq!(plat.sha256.len(), 64);
                    assert!(plat.bytes > 500_000);
                    assert!(!plat.sentinel_libraries.is_empty());
                }
            }
        }
    }

    #[test]
    fn provider_binary_inspection_distinguishes_cuda12_and_cuda13() {
        assert_eq!(detect_cuda_variant_from_provider_bytes(b"MZ...cublas64_12.dll\0"), Some("cuda12"));
        assert_eq!(detect_cuda_variant_from_provider_bytes(b"MZ...cublas64_13.dll\0"), Some("cuda13"));
        assert_eq!(detect_cuda_variant_from_provider_bytes(b"\x7fELF...libcublas.so.13\0"), Some("cuda13"));
        assert_eq!(detect_cuda_variant_from_provider_bytes(b"no_cuda_imports"), None);
    }

    #[test]
    fn provider_variant_is_recorded_and_reused_until_the_binary_changes() {
        let dir = std::env::temp_dir().join(format!("cluaiz-provider-variant-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let provider = dir.join("onnxruntime_providers_cuda.dll");
        std::fs::write(&provider, b"MZ...cublas64_13.dll\0").unwrap();
        assert_eq!(provider_variant(&provider, &dir), Some("cuda13"));
        let record = std::fs::read_to_string(dir.join(VARIANT_RECORD)).unwrap();
        assert!(record.starts_with("cuda13\t"));
        // A different binary (another size) is read again
        std::fs::write(&provider, b"MZ......cublas64_12.dll\0").unwrap();
        assert_eq!(provider_variant(&provider, &dir), Some("cuda12"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sentinel_detection_requires_non_empty_file() {
        let dir = std::env::temp_dir().join(format!("cluaiz-cudnn-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = platform_source("windows-x86_64").unwrap();
        assert!(!is_cudnn_in_drivers_dir(&dir, src));
        let sentinel = dir.join(&src.sentinel_library);
        std::fs::write(&sentinel, b"").unwrap();
        assert!(!is_cudnn_in_drivers_dir(&dir, src));
        std::fs::write(&sentinel, b"MZ-real-dll-bytes").unwrap();
        assert!(is_cudnn_in_drivers_dir(&dir, src));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_cudnn_ready_skips_when_no_nvidia_gpu() {
        let dir = std::env::temp_dir().join(format!("cluaiz-cudnn-nogpu-{}", std::process::id()));
        assert!(!ensure_cudnn_ready(&dir, false).unwrap());
        assert!(!dir.exists());
    }

    #[test]
    fn archive_verification_and_zip_extraction_only_keep_bin_runtime_dlls() {
        let dir = std::env::temp_dir().join(format!("cluaiz-cudnn-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("test_cudnn.zip");
        {
            let mut zip = zip::ZipWriter::new(File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("cudnn-archive/bin/cudnn64_9.dll", opts).unwrap();
            zip.write_all(b"cudnn64_9_binary_payload").unwrap();
            zip.start_file("cudnn-archive/bin/cudnn_graph64_9.dll", opts).unwrap();
            zip.write_all(b"cudnn_graph64_9_payload").unwrap();
            zip.start_file("cudnn-archive/include/cudnn.h", opts).unwrap();
            zip.write_all(b"#define CUDNN_MAJOR 9").unwrap();
            zip.start_file("cudnn-archive/lib/x64/cudnn.lib", opts).unwrap();
            zip.write_all(b"import_lib").unwrap();
            zip.finish().unwrap();
        }
        let bytes = std::fs::read(&zip_path).unwrap();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        verify_archive_file(&zip_path, bytes.len() as u64, &sha).unwrap();
        assert!(verify_archive_file(&zip_path, bytes.len() as u64 + 1, &sha).is_err());
        assert!(verify_archive_file(&zip_path, bytes.len() as u64, &"0".repeat(64)).is_err());
        let out_dir = dir.join("drivers");
        let src = platform_source("windows-x86_64").unwrap();
        assert_eq!(extract_cudnn_from_zip(&zip_path, &out_dir, src).unwrap(), 2);
        assert!(is_cudnn_in_drivers_dir(&out_dir, src));
        assert!(out_dir.join("cudnn_graph64_9.dll").exists());
        assert!(!out_dir.join("cudnn.h").exists());
        assert!(!out_dir.join("cudnn.lib").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore]
    fn live_provision_and_verify_cuda_ep() {
        let drivers_dir = default_drivers_dir();
        eprintln!("=== [LIVE GPU VERIFICATION] Target drivers_dir: {:?} ===", drivers_dir);
        let provisioned = provision_cuda_runtime_at_setup()
            .expect("provision_cuda_runtime_at_setup must succeed on NVIDIA hardware");
        assert!(provisioned, "Expected NVIDIA GPU to be detected and CUDA+cuDNN9 provisioned");
        assert!(is_cuda_runtime_available_in(&drivers_dir));

        let mut files: Vec<(String, u64)> = std::fs::read_dir(&drivers_dir)
            .expect("drivers_dir must exist")
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some((e.file_name().to_string_lossy().into_owned(), meta.len()))
            })
            .collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        eprintln!("=== [LIVE GPU VERIFICATION] Installed files in {:?} ({}) ===", drivers_dir, files.len());
        for (name, bytes) in &files {
            eprintln!("  {:<42} {:>12} bytes", name, bytes);
        }

        // Verify strict ONNX Runtime CUDA Execution Provider session creation + forward pass on GPU
        const IDENTITY_GRAPH: &[u8] = &[
            8, 8, 58, 55, 10, 16, 10, 1, b'x', 18, 1, b'y', 34, 8, b'I', b'd', b'e', b'n', b't', b'i',
            b't', b'y', 18, 1, b'g', 90, 15, 10, 1, b'x', 18, 10, 10, 8, 8, 1, 18, 4, 10, 2, 8, 1, 98,
            15, 10, 1, b'y', 18, 10, 10, 8, 8, 1, 18, 4, 10, 2, 8, 1, 66, 2, 16, 13,
        ];
        ort::init().with_name("cluaiz_live_cuda_verify").commit();
        let mut session = ort::session::Session::builder()
            .expect("Session::builder")
            .with_execution_providers([ort::ep::CUDA::default().build().error_on_failure()])
            .expect("Strict CUDA EP registration must succeed with provisioned DLLs")
            .commit_from_memory(IDENTITY_GRAPH)
            .expect("Strict CUDA EP session commit must succeed on physical NVIDIA GPU");
        let input = ort::value::Tensor::from_array(([1usize], vec![7.5f32])).unwrap();
        let outputs = session.run(ort::inputs!["x" => input]).expect("CUDA EP forward pass");
        assert_eq!(outputs["y"].try_extract_tensor::<f32>().unwrap().1, &[7.5]);
        eprintln!("=== [LIVE GPU VERIFICATION] Strict CUDA Execution Provider forward pass PASSED ===");

        let models_dir = cluaiz_core::EnvironmentManager::current().models_dir();
        let onnx_files = crate::OnnxEngine::scan_onnx_files_recursive(&models_dir);
        for onnx_path in &onnx_files {
            let path_str = onnx_path.to_string_lossy().to_lowercase();
            if path_str.contains("kokoro") || path_str.contains("tts") || path_str.contains("bge_m3") {
                eprintln!("=== [LIVE GPU VERIFICATION] Committing strict CUDA EP session for {:?} ===", onnx_path);
                let t0 = std::time::Instant::now();
                let mut model_sess = ort::session::Session::builder()
                    .expect("Session::builder")
                    .with_execution_providers([ort::ep::CUDA::default().build().error_on_failure()])
                    .expect("Strict CUDA EP registration")
                    .commit_from_file(onnx_path)
                    .unwrap_or_else(|e| panic!("Strict CUDA EP commit failed for {:?}: {:?}", onnx_path, e));
                eprintln!("=== [LIVE GPU VERIFICATION] Committed {:?} on CUDA EP in {:?} ===", onnx_path.file_name().unwrap_or_default(), t0.elapsed());
                if path_str.contains("kokoro") {
                    let mut eng = crate::OnnxEngine::new().expect("OnnxEngine::new");
                    eng.model_dir = onnx_path.parent().map(|p| p.to_path_buf());
                    let t_syn = std::time::Instant::now();
                    let wav_uri = crate::audio::tts::tts_router::route_tts_with(&eng, &mut model_sess, "NVIDIA CUDA and cuDNN 9 speech synthesis verified on GPU.", Some("af_heart"), 1.0, None).expect("Kokoro CUDA GPU synthesis");
                    assert!(wav_uri.starts_with("data:audio/wav;base64,") && wav_uri.len() > 10_000);
                    eprintln!("=== [LIVE GPU VERIFICATION] Kokoro TTS synthesized {} base64 chars on NVIDIA CUDA GPU in {:?} ===", wav_uri.len(), t_syn.elapsed());
                }
            }
        }
    }
}

