//! Explicit packaged-provider assets. No PATH search, runtime download or model lookup.

use anyhow::{bail, Context, Result};
use cluaiz_core::EnvironmentManager;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const MANIFEST_NAME: &str = "native-assets.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetFile {
    pub path: PathBuf,
    pub sha256: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrAssets {
    pub library: String,
    pub data_directory: PathBuf,
    pub languages: String,
    pub deadline_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfAssets {
    pub library: String,
    /// Rendering resolution, independent of a model's input size.
    pub dpi: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoAssets {
    pub library: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetManifest {
    pub schema_version: u32,
    pub os: String,
    pub arch: String,
    pub files: BTreeMap<String, AssetFile>,
    pub ocr: Option<OcrAssets>,
    pub pdf: Option<PdfAssets>,
    pub video: Option<VideoAssets>,
}

pub struct NativeAssets {
    pub root: PathBuf,
    pub manifest: AssetManifest,
    pub identity: String,
}

impl NativeAssets {
    pub fn from_environment() -> Result<Self> {
        Self::read(&EnvironmentManager::current().engine_dir().join("media"))
    }

    pub fn read(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .context("Packaged native media directory is unavailable")?;
        let file = std::fs::File::open(root.join(MANIFEST_NAME))
            .context("Packaged native media manifest is unavailable")?;
        // Manifests are package metadata, not arbitrary-sized attachment input.
        let mut bytes = Vec::new();
        file.take(64 * cluaiz_core::KIB + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * cluaiz_core::KIB as usize {
            bail!("Native media manifest is too large");
        }
        let manifest: AssetManifest = serde_json::from_slice(&bytes)?;
        if manifest.schema_version != 1
            || manifest.os != std::env::consts::OS
            || manifest.arch != std::env::consts::ARCH
        {
            bail!("Native media package schema or platform does not match this engine");
        }
        let mut assets = Self {
            root,
            manifest,
            identity: format!("{:x}", Sha256::digest(&bytes)),
        };
        // Invalidate resident providers when a declared asset disappears/changes.
        // Hash verification still occurs before a replacement library is loaded.
        let mut snapshot = Sha256::new();
        for asset in assets.manifest.files.values() {
            let path = assets.path(&asset.path)?;
            let metadata = path.metadata()?;
            if !metadata.is_file() {
                bail!("Native manifest assets must be files");
            }
            snapshot.update(path.to_string_lossy().as_bytes());
            snapshot.update(metadata.len().to_le_bytes());
            snapshot.update(
                metadata
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
                    .to_le_bytes(),
            );
        }
        assets.identity = format!(
            "{}:{}:{:x}",
            assets.root.display(),
            assets.identity,
            snapshot.finalize()
        );
        Ok(assets)
    }

    /// Check every packaged dependency before executing any native initializer.
    /// Hashes detect package damage; trust in the manifest comes from the installer.
    pub fn verify(&self) -> Result<()> {
        for (name, asset) in &self.manifest.files {
            if asset.sha256.len() != 64 || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("Native asset {name} has an invalid SHA-256 digest");
            }
            let path = self.path(&asset.path)?;
            let mut file = std::fs::File::open(&path)?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hash.update(&buffer[..read]);
            }
            if format!("{:x}", hash.finalize()) != asset.sha256.to_ascii_lowercase() {
                bail!("Native asset {name} failed SHA-256 verification");
            }
        }
        Ok(())
    }

    pub fn path(&self, relative: &Path) -> Result<PathBuf> {
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("Native asset paths must be relative package paths without traversal");
        }
        let path = self.root.join(relative).canonicalize()?;
        if !path.starts_with(&self.root) {
            bail!("Native asset resolves outside its package");
        }
        Ok(path)
    }

    pub fn library(&self, key: &str) -> Result<PathBuf> {
        self.path(
            &self
                .manifest
                .files
                .get(key)
                .context("Native library is not listed in the package manifest")?
                .path,
        )
    }
}

/// One shared loader for explicit canonical libraries. Dependencies may come from
/// the library's package directory or the OS system directory, never the CWD/PATH.
pub(crate) fn load_library(path: &Path) -> Result<libloading::Library> {
    // SAFETY: callers verified trusted package hashes and the selected platform.
    // Loading executes native initializers; function ABIs follow provider headers.
    // The returned owner remains alive as long as every loaded function/handle.
    unsafe {
        #[cfg(windows)]
        {
            use libloading::os::windows::{
                Library, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
            };
            Ok(Library::load_with_flags(
                path,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )?
            .into())
        }
        #[cfg(not(windows))]
        {
            Ok(libloading::Library::new(path)?)
        }
    }
}

/// # Safety
/// Caller must use the exact documented C ABI and retain the library owner.
pub(crate) unsafe fn symbol<T: Copy>(library: &libloading::Library, name: &[u8]) -> Result<T> {
    Ok(*library.get::<T>(name)?)
}
