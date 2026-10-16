//! Bounded local, HTTP and base64 source preparation with owned temporary assets.

use crate::formats::{FormatRegistry, MediaFormat};
use anyhow::{bail, Context, Result};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE},
    Engine,
};
use cluaiz_core::{media::MediaSettings, media_limits::MediaLimits, EnvironmentManager};
use reqwest::Client;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tempfile::TempPath;

enum SourceStorage {
    Local(PathBuf),
    Temporary(TempPath),
}

/// Clones share temporary-file ownership; the last owner removes only generated assets.
#[derive(Clone)]
pub struct ResolvedSource {
    storage: Arc<SourceStorage>,
    pub format: MediaFormat,
    pub input_bytes: u64,
}

impl ResolvedSource {
    /// Path available for the lifetime of this source or any of its clones.
    pub fn path(&self) -> &Path {
        match self.storage.as_ref() {
            SourceStorage::Local(path) => path,
            SourceStorage::Temporary(path) => path.as_ref(),
        }
    }

    /// Reads using the same byte ceiling, including files that grew after resolution.
    pub async fn read_bytes(&self, limits: MediaLimits) -> Result<Vec<u8>> {
        // Keep a generated asset owned until the blocking read finishes, even if
        // the awaiting request is cancelled before the worker returns.
        let source = self.clone();
        tokio::task::spawn_blocking(move || read_local_bytes(source.path(), limits)).await?
    }
}

/// Reusable HTTP client and format registry. Filesystem access policy is supplied by the caller.
pub struct SourceResolver {
    client: Client,
    temp_dir: PathBuf,
    limits: MediaLimits,
    formats: FormatRegistry,
    video: cluaiz_core::media::VideoSettings,
    cancellation: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) progress: Option<cluaiz_core::processing::ProcessingReporter>,
}

impl SourceResolver {
    pub fn with_progress(
        mut self,
        progress: Option<cluaiz_core::processing::ProcessingReporter>,
    ) -> Self {
        self.progress = progress;
        self
    }
    pub fn video_settings(&self) -> cluaiz_core::media::VideoSettings {
        self.video
    }

    pub fn cancellation(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.cancellation.clone()
    }

    /// Share a request's real cancellation signal with blocking preparation.
    pub fn with_cancellation(mut self, signal: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.cancellation = signal;
        self
    }

    /// The configured policy used by this resolver and its preparation consumers.
    pub fn limits(&self) -> MediaLimits {
        self.limits
    }

    /// Resolves generated-file placement through the existing environment manager.
    pub fn from_environment(settings: MediaSettings) -> Result<Self> {
        Self::new(
            EnvironmentManager::current().local_dir.join("temp_media"),
            settings,
            FormatRegistry::default(),
        )
    }

    /// Accepts explicit storage/configuration for portable applications and isolated tests.
    pub fn new(
        temp_dir: PathBuf,
        settings: MediaSettings,
        formats: FormatRegistry,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_millis(settings.http_timeout_ms.get()))
            .build()?;
        Ok(Self {
            client,
            temp_dir,
            limits: settings.limits,
            formats,
            video: settings.video,
            cancellation: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress: None,
        })
    }

    /// Prepares a source; authorization runs on the canonical local path before reading it.
    pub async fn resolve(
        &self,
        input: &str,
        authorize_local: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<ResolvedSource> {
        if input.starts_with("data:") {
            let (bytes, hint) = decode_data_uri(input, self.limits)?;
            return self.materialize(bytes, hint).await;
        }
        if input.starts_with("http://") || input.starts_with("https://") {
            let mut response = self.client.get(input).send().await?.error_for_status()?;
            if let Some(length) = response.content_length() {
                self.limits.check_input_bytes(length)?;
            }
            let hint = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                let next = bytes
                    .len()
                    .checked_add(chunk.len())
                    .context("Media byte count overflow")?;
                self.limits.check_input_bytes(u64::try_from(next)?)?;
                bytes.try_reserve(chunk.len())?;
                bytes.extend_from_slice(&chunk);
            }
            return self.materialize(bytes, hint.as_deref()).await;
        }
        let path = local_path(input)?;
        let canonical = tokio::fs::canonicalize(path).await?;
        authorize_local(&canonical)?;
        let bytes = bounded_read(&canonical, self.limits).await?;
        let hint = mime_guess::from_path(&canonical).first_raw();
        let format = self.formats.detect(&bytes, hint)?;
        Ok(ResolvedSource {
            storage: Arc::new(SourceStorage::Local(canonical)),
            input_bytes: u64::try_from(bytes.len())?,
            format,
        })
    }

    async fn materialize(&self, bytes: Vec<u8>, hint: Option<&str>) -> Result<ResolvedSource> {
        self.limits.check_input_bytes(u64::try_from(bytes.len())?)?;
        if bytes.is_empty() {
            bail!("Media source is empty");
        }
        let format = self.formats.detect(&bytes, hint)?;
        let input_bytes = u64::try_from(bytes.len())?;
        let suffix = format!(".{}", format.extension);
        let dir = self.temp_dir.clone();
        let path = tokio::task::spawn_blocking(move || -> Result<TempPath> {
            std::fs::create_dir_all(&dir)?;
            let mut file = tempfile::Builder::new().suffix(&suffix).tempfile_in(dir)?;
            file.write_all(&bytes)?;
            Ok(file.into_temp_path())
        })
        .await??;
        Ok(ResolvedSource {
            storage: Arc::new(SourceStorage::Temporary(path)),
            format,
            input_bytes,
        })
    }
}

async fn bounded_read(path: &Path, limits: MediaLimits) -> Result<Vec<u8>> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || read_local_bytes(&path, limits)).await?
}

/// Blocking source read for existing synchronous backend callers.
/// HTTP must first be prepared with SourceResolver; authorization precedes local reads.
pub fn read_local_or_inline(
    input: &str,
    limits: MediaLimits,
    authorize_local: impl FnOnce(&Path) -> Result<()>,
) -> Result<Vec<u8>> {
    if input.starts_with("data:") {
        let (bytes, hint) = decode_data_uri(input, limits)?;
        FormatRegistry::default().detect(&bytes, hint)?;
        return Ok(bytes);
    }
    let path = std::fs::canonicalize(local_path(input)?)?;
    authorize_local(&path)?;
    read_local_bytes(&path, limits)
}

/// Returns a local locator for request-boundary authorization. Remote/inline
/// sources have no local input path; generated temporary files are owned internally.
pub fn local_source_path(input: &str) -> Result<Option<PathBuf>> {
    if input.starts_with("data:") || input.starts_with("http://") || input.starts_with("https://") {
        Ok(None)
    } else {
        local_path(input).map(Some)
    }
}

fn local_path(input: &str) -> Result<PathBuf> {
    if input.starts_with("file:") {
        return reqwest::Url::parse(input)?
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Invalid local file URI"));
    }
    if input.contains("://") {
        bail!("Remote media requires asynchronous SourceResolver preparation");
    }
    Ok(PathBuf::from(input))
}

fn decode_data_uri(input: &str, limits: MediaLimits) -> Result<(Vec<u8>, Option<&str>)> {
    let (header, encoded) = input.split_once(',').context("Malformed media data URI")?;
    if !header.ends_with(";base64") || !encoded.len().is_multiple_of(4) {
        bail!("Media data URI must contain padded base64");
    }
    let padding = encoded.bytes().rev().take_while(|b| *b == b'=').count();
    if padding > 2 {
        bail!("Invalid base64 padding");
    }
    let expected = (encoded.len() / 4)
        .checked_mul(3)
        .and_then(|n| n.checked_sub(padding))
        .context("Invalid base64 size")?;
    limits.check_input_bytes(u64::try_from(expected)?)?;
    let bytes = STANDARD
        .decode(encoded)
        .or_else(|_| URL_SAFE.decode(encoded))?;
    if bytes.is_empty() {
        bail!("Media source is empty");
    }
    let hint = header
        .strip_prefix("data:")
        .and_then(|h| h.split(';').next());
    Ok((bytes, hint))
}

fn read_local_bytes(path: &Path, limits: MediaLimits) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        bail!("Media source must be a regular file");
    }
    limits.check_input_bytes(metadata.len())?;
    let mut bytes = Vec::new();
    file.take(limits.max_input_bytes().saturating_add(1))
        .read_to_end(&mut bytes)?;
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    if bytes.is_empty() {
        bail!("Media source is empty");
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
