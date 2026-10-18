//! Runtime provisioning for NVIDIA cuDNN 9 shared libraries required by the ONNX Runtime CUDA
//! Execution Provider (`onnxruntime_providers_cuda`).
//!
//! When an NVIDIA GPU is present and the cuDNN 9 sentinel library (`cudnn64_9.dll` on Windows,
//! `libcudnn.so.9` on Linux) is missing from `<cluaiz_home>/engine/drivers/` and the system library
//! path, this module downloads the official NVIDIA cuDNN 9 redistributable archive pinned in
//! `data/cudnn_sources.json`, verifies its exact byte size and SHA-256 digest, and extracts the
//! runtime shared libraries into `<cluaiz_home>/engine/drivers/`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const CUDNN_SOURCES_JSON: &str = include_str!("data/cudnn_sources.json");

#[derive(Debug, Clone, Deserialize)]
pub struct CudnnManifest {
    pub version: String,
    pub cuda_variant: String,
    pub base_url: String,
    pub platforms: Vec<CudnnPlatformSource>,
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
    {
        return Some("windows-x86_64");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return Some("linux-x86_64");
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Some("linux-aarch64");
    }
    #[allow(unreachable_code)]
    None
}

/// Resolves the `CudnnPlatformSource` for a given platform key (e.g. `"windows-x86_64"`).
pub fn platform_source(platform_key: &str) -> Option<&'static CudnnPlatformSource> {
    manifest()?
        .platforms
        .iter()
        .find(|entry| entry.platform == platform_key)
}

/// Default `<cluaiz_home>/engine/drivers` directory.
pub fn default_drivers_dir() -> PathBuf {
    cluaiz_core::EnvironmentManager::current()
        .engine_dir()
        .join("drivers")
}

/// Checks whether the platform's cuDNN 9 sentinel library exists and is non-empty inside
/// `drivers_dir`.
pub fn is_cudnn_in_drivers_dir(drivers_dir: &Path, source: &CudnnPlatformSource) -> bool {
    let sentinel = drivers_dir.join(&source.sentinel_library);
    std::fs::metadata(sentinel)
        .map(|meta| meta.is_file() && meta.len() > 0)
        .unwrap_or(false)
}

/// Checks whether the platform's cuDNN 9 sentinel library is available either in `drivers_dir` or
/// in the operating system's dynamic library search paths.
pub fn is_cudnn_available_in(drivers_dir: &Path) -> bool {
    let Some(key) = current_platform_key() else {
        return false;
    };
    let Some(source) = platform_source(key) else {
        return false;
    };
    if is_cudnn_in_drivers_dir(drivers_dir, source) {
        return true;
    }
    is_sentinel_on_system_path(&source.sentinel_library)
}

fn is_sentinel_on_system_path(sentinel_library: &str) -> bool {
    #[cfg(windows)]
    {
        if let Some(path_os) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_os) {
                let candidate = dir.join(sentinel_library);
                if std::fs::metadata(&candidate)
                    .map(|m| m.is_file() && m.len() > 0)
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }
        false
    }
    #[cfg(not(windows))]
    {
        if let Some(ld_os) = std::env::var_os("LD_LIBRARY_PATH") {
            for dir in std::env::split_paths(&ld_os) {
                let candidate = dir.join(sentinel_library);
                if std::fs::metadata(&candidate)
                    .map(|m| m.is_file() && m.len() > 0)
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }
        for std_dir in [
            "/usr/local/cuda/lib64",
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
            "/usr/lib64",
        ] {
            let candidate = Path::new(std_dir).join(sentinel_library);
            if std::fs::metadata(&candidate)
                .map(|m| m.is_file() && m.len() > 0)
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }
}

/// Ensures `drivers_dir` is registered on `PATH` (Windows) so `LoadLibrary` inside
/// `onnxruntime_providers_cuda` can resolve `cudnn64_9.dll`.
pub fn register_drivers_dir_in_path(drivers_dir: &Path) {
    #[cfg(windows)]
    {
        if !drivers_dir.exists() {
            return;
        }
        let current_path = std::env::var_os("PATH").unwrap_or_default();
        let already_present = std::env::split_paths(&current_path).any(|p| p == drivers_dir);
        if !already_present {
            let mut new_path = drivers_dir.as_os_str().to_os_string();
            if !current_path.is_empty() {
                new_path.push(";");
                new_path.push(current_path);
            }
            std::env::set_var("PATH", new_path);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = drivers_dir;
    }
}

/// Provisions cuDNN 9 into the default `<cluaiz_home>/engine/drivers` directory if an NVIDIA GPU
/// is present and cuDNN 9 is not yet installed.
pub fn ensure_cudnn_ready_default() -> Result<bool> {
    let hw = cluaiz_hw::HardwareProber::current();
    let has_nvidia = hw
        .gpus
        .iter()
        .any(|g| g.vendor.to_uppercase().contains("NVIDIA"));
    let drivers_dir = default_drivers_dir();
    let ready = ensure_cudnn_ready(&drivers_dir, has_nvidia)?;
    if ready {
        register_drivers_dir_in_path(&drivers_dir);
    }
    Ok(ready)
}

/// Best-effort pre-flight hook invoked before registering the ONNX Runtime CUDA Execution
/// Provider. If cuDNN 9 is already present, ensures `drivers_dir` is on `PATH`. If missing on an
/// NVIDIA system, attempts a single automatic download per process and logs a warning on failure
/// so the caller can continue to DirectML/CPU fallback.
pub fn ensure_cudnn_ready_for_cuda() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static FAILED_ONCE: AtomicBool = AtomicBool::new(false);

    let drivers_dir = default_drivers_dir();
    if is_cudnn_available_in(&drivers_dir) {
        register_drivers_dir_in_path(&drivers_dir);
        return;
    }
    if FAILED_ONCE.load(Ordering::Relaxed) {
        return;
    }
    match ensure_cudnn_ready_default() {
        Ok(_) => {}
        Err(error) => {
            FAILED_ONCE.store(true, Ordering::Relaxed);
            tracing::warn!(
                %error,
                "Automatic cuDNN 9 provisioning did not complete; CUDA Execution Provider may fall back to DirectML/CPU"
            );
        }
    }
}

/// Checks if cuDNN 9 is ready for `drivers_dir` when `has_nvidia_gpu` is true, and downloads +
/// extracts it from the official NVIDIA redistributable if missing.
pub fn ensure_cudnn_ready(drivers_dir: &Path, has_nvidia_gpu: bool) -> Result<bool> {
    if !has_nvidia_gpu {
        return Ok(false);
    }
    let Some(key) = current_platform_key() else {
        return Ok(false);
    };
    let Some(manifest) = manifest() else {
        bail!("cuDNN manifest is unavailable");
    };
    let Some(source) = platform_source(key) else {
        return Ok(false);
    };

    if is_cudnn_in_drivers_dir(drivers_dir, source)
        || is_sentinel_on_system_path(&source.sentinel_library)
    {
        return Ok(true);
    }

    if std::env::var("CLUAIZ_DISABLE_CUDNN_AUTODOWNLOAD")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        tracing::info!(
            "Skipping automatic cuDNN 9 download because CLUAIZ_DISABLE_CUDNN_AUTODOWNLOAD is set"
        );
        return Ok(false);
    }

    std::fs::create_dir_all(drivers_dir)
        .with_context(|| format!("failed to create drivers directory {:?}", drivers_dir))?;

    let url = format!("{}{}", manifest.base_url, source.relative_path);
    let archive_ext = if source.relative_path.ends_with(".zip") {
        "zip"
    } else {
        "tar.xz"
    };
    let part_archive = drivers_dir.join(format!(
        "cudnn-{}-{}.part.{}",
        manifest.version,
        std::process::id(),
        archive_ext
    ));

    tracing::info!(
        version = %manifest.version,
        platform = %source.platform,
        bytes = source.bytes,
        "Downloading NVIDIA cuDNN 9 runtime archive for ONNX CUDA Execution Provider"
    );

    let download_result = (|| -> Result<()> {
        download_to_file(&url, &part_archive, source.bytes)?;
        verify_archive_file(&part_archive, source.bytes, &source.sha256)?;
        if archive_ext == "zip" {
            extract_cudnn_from_zip(&part_archive, drivers_dir, source)?;
        } else {
            extract_cudnn_from_tar_xz(&part_archive, drivers_dir, source)?;
        }
        Ok(())
    })();

    let _ = std::fs::remove_file(&part_archive);
    download_result?;

    if !is_cudnn_in_drivers_dir(drivers_dir, source) {
        bail!(
            "cuDNN archive extracted but sentinel library '{}' was not found in {:?}",
            source.sentinel_library,
            drivers_dir
        );
    }

    let _ = std::fs::write(drivers_dir.join("cudnn9.ready"), &manifest.version);
    tracing::info!(
        version = %manifest.version,
        drivers_dir = ?drivers_dir,
        "NVIDIA cuDNN 9 runtime libraries installed into engine drivers directory"
    );
    Ok(true)
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

    let file = File::create(dest)
        .with_context(|| format!("failed to create temporary archive {:?}", dest))?;
    let mut writer = BufWriter::new(file);
    let mut reader = response.take(expected_bytes + 1);
    let copied = std::io::copy(&mut reader, &mut writer)
        .with_context(|| format!("failed while streaming download from {url}"))?;
    writer.flush()?;
    if copied != expected_bytes {
        bail!(
            "downloaded size mismatch for {url}: expected {expected_bytes} bytes, got {copied} bytes"
        );
    }
    Ok(())
}

/// Verifies that an archive on disk matches both the pinned byte count and SHA-256 digest.
pub fn verify_archive_file(path: &Path, expected_bytes: u64, expected_sha256: &str) -> Result<()> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("failed to stat archive {:?}", path))?;
    if meta.len() != expected_bytes {
        bail!(
            "archive byte size mismatch: expected {expected_bytes}, got {}",
            meta.len()
        );
    }
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
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
        bail!(
            "SHA-256 mismatch for {:?}: expected {}, got {}",
            path,
            expected_sha256,
            actual_sha
        );
    }
    Ok(())
}

/// Extracts matching cuDNN runtime shared libraries (`bin/cudnn*.dll`) from a verified ZIP archive
/// into `drivers_dir` using atomic `.part` renames.
pub fn extract_cudnn_from_zip(
    archive_path: &Path,
    drivers_dir: &Path,
    source: &CudnnPlatformSource,
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
        let enclosed = match entry.enclosed_name() {
            Some(path) => path.to_path_buf(),
            None => continue,
        };
        let Some(file_name) = enclosed.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let in_bin_or_lib = enclosed.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s.eq_ignore_ascii_case("bin") || s.eq_ignore_ascii_case("lib")
        });
        if !in_bin_or_lib
            || !file_name.starts_with(&source.library_prefix)
            || !file_name.contains(&source.library_extension)
        {
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
        bail!("no cuDNN runtime libraries matched inside {:?}", archive_path);
    }
    Ok(extracted_count)
}

fn extract_cudnn_from_tar_xz(
    archive_path: &Path,
    drivers_dir: &Path,
    source: &CudnnPlatformSource,
) -> Result<usize> {
    let temp_extract = drivers_dir.join(format!("cudnn-extract-{}", std::process::id()));
    std::fs::create_dir_all(&temp_extract)?;

    let status = std::process::Command::new("tar")
        .arg("-xJf")
        .arg(archive_path)
        .arg("-C")
        .arg(&temp_extract)
        .status()
        .context("failed to invoke system 'tar' to extract cuDNN .tar.xz archive")?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&temp_extract);
        bail!("tar exited with non-zero status: {status}");
    }

    let mut extracted_count = 0usize;
    let mut stack = vec![temp_extract.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !file_name.starts_with(&source.library_prefix)
                || !file_name.contains(&source.library_extension)
            {
                continue;
            }
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.is_file() && meta.len() > 0 {
                    let target = drivers_dir.join(file_name);
                    std::fs::copy(&path, &target).with_context(|| {
                        format!("failed to copy {:?} to {:?}", path, target)
                    })?;
                    extracted_count += 1;
                }
            }
        }
    }

    let _ = std::fs::remove_dir_all(&temp_extract);
    if extracted_count == 0 {
        bail!("no cuDNN shared libraries found in {:?}", archive_path);
    }
    Ok(extracted_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_and_pins_all_supported_platforms() {
        let m = manifest().expect("cudnn_sources.json must parse");
        assert_eq!(m.version, "9.8.0.87");
        assert_eq!(m.cuda_variant, "cuda12");
        assert!(m.base_url.starts_with("https://developer.download.nvidia.com/"));
        for key in ["windows-x86_64", "linux-x86_64", "linux-aarch64"] {
            let src = platform_source(key).unwrap_or_else(|| panic!("missing platform {key}"));
            assert_eq!(src.sha256.len(), 64, "invalid sha256 for {key}");
            assert!(src.bytes > 100_000_000, "unexpected byte size for {key}");
            assert!(!src.sentinel_library.is_empty());
        }
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
        let ready = ensure_cudnn_ready(&dir, false).unwrap();
        assert!(!ready);
        assert!(!dir.exists());
    }

    #[test]
    fn archive_verification_and_zip_extraction_only_keep_bin_runtime_dlls() {
        let dir = std::env::temp_dir().join(format!("cluaiz-cudnn-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let zip_path = dir.join("test_cudnn.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            zip.start_file("cudnn-archive/bin/cudnn64_9.dll", opts)
                .unwrap();
            zip.write_all(b"cudnn64_9_binary_payload").unwrap();

            zip.start_file("cudnn-archive/bin/cudnn_graph64_9.dll", opts)
                .unwrap();
            zip.write_all(b"cudnn_graph64_9_payload").unwrap();

            zip.start_file("cudnn-archive/include/cudnn.h", opts)
                .unwrap();
            zip.write_all(b"#define CUDNN_MAJOR 9").unwrap();

            zip.start_file("cudnn-archive/lib/x64/cudnn.lib", opts)
                .unwrap();
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
        let count = extract_cudnn_from_zip(&zip_path, &out_dir, src).unwrap();
        assert_eq!(count, 2);
        assert!(is_cudnn_in_drivers_dir(&out_dir, src));
        assert!(out_dir.join("cudnn_graph64_9.dll").exists());
        assert!(!out_dir.join("cudnn.h").exists());
        assert!(!out_dir.join("cudnn.lib").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
