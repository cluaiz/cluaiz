//! Human-selected input only. Detection/decoding remains in cluaiz-media; no file is executed.

use super::Tools;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE},
    Engine,
};
use cluaiz_core::{
    media::{DecodedImage, MediaSettings, PreparedPart},
    ChatMessage,
};
use cluaiz_media::{formats::FormatRegistry, source::SourceResolver};
use color_eyre::{eyre::eyre, Result};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub enum Payload {
    Source(String),
    Prepared(Vec<PreparedPart>),
}
pub struct Attachment {
    pub label: String,
    pub payload: Payload,
}
impl Attachment {
    pub fn retained_bytes(&self) -> usize {
        self.label.len().saturating_add(match &self.payload {
            Payload::Source(source) => source.len(),
            Payload::Prepared(parts) => parts.iter().map(PreparedPart::retained_bytes).sum(),
        })
    }
}
pub struct Input {
    pub text: String,
    pub attachments: Vec<Attachment>,
}
pub enum Paste {
    Text(String),
    Files(Vec<Attachment>),
}
pub enum Ingress {
    Clipboard,
    Text(String),
}

/// Fast, filesystem-free check. Ordinary pasted prose is inserted synchronously by the editor.
pub fn candidate(text: &str) -> bool {
    let text = text.trim().strip_prefix("& ").unwrap_or(text.trim()).trim();
    text.starts_with("data:")
        || Path::new(text.trim_matches(['\'', '"'])).is_absolute()
        || encoded_format(text).is_some()
}

fn encoded_format(text: &str) -> Option<cluaiz_media::formats::MediaFormat> {
    const PROBE_CHARS: usize = 64;
    const MIN_ENCODED_SIGNATURE: usize = 16;
    if text.len() < MIN_ENCODED_SIGNATURE || !text.is_ascii() {
        return None;
    }
    let prefix: String = text
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .take(PROBE_CHARS)
        .collect();
    let bytes = STANDARD
        .decode(&prefix)
        .or_else(|_| URL_SAFE.decode(&prefix))
        .ok()?;
    FormatRegistry::default()
        .detect(&bytes, None)
        .ok()
        .filter(|format| format.content_detected)
}

pub fn ingest(input: Ingress, root: &Path, settings: MediaSettings) -> Result<Paste> {
    match input {
        Ingress::Text(text) => from_text(text, root, settings),
        Ingress::Clipboard => {
            let mut clipboard =
                arboard::Clipboard::new().map_err(|e| eyre!("Clipboard unavailable: {e}"))?;
            if let Ok(files) = clipboard.get().file_list() {
                if !files.is_empty() {
                    return files_from_paths(files, root);
                }
            }
            if let Ok(image) = clipboard.get_image() {
                let width = u32::try_from(image.width)?;
                let height = u32::try_from(image.height)?;
                let expected = image
                    .width
                    .checked_mul(image.height)
                    .and_then(|n| n.checked_mul(4))
                    .ok_or_else(|| eyre!("Clipboard image geometry overflow"))?;
                if expected != image.bytes.len() {
                    return Err(eyre!("Clipboard returned invalid RGBA pixels"));
                }
                let rgb_bytes = expected / 4 * cluaiz_core::media::RGB8_CHANNELS;
                settings.limits.check_decoded_bytes(u64::try_from(
                    expected
                        .checked_add(rgb_bytes)
                        .ok_or_else(|| eyre!("Clipboard image byte count overflow"))?,
                )?)?;
                let pixels: Vec<u8> = image
                    .bytes
                    .chunks_exact(4)
                    .flat_map(|pixel| pixel[..3].iter().copied())
                    .collect();
                drop(image);
                let image = DecodedImage::new(width, height, Arc::from(pixels))?;
                return Ok(Paste::Files(vec![Attachment {
                    label: format!("Clipboard image · {width}×{height}"),
                    payload: Payload::Prepared(vec![PreparedPart::Image { image }]),
                }]));
            }
            let text = clipboard
                .get_text()
                .map_err(|e| eyre!("Clipboard has no supported image, files or text: {e}"))?;
            from_text(text, root, settings)
        }
    }
}

fn from_text(text: String, root: &Path, settings: MediaSettings) -> Result<Paste> {
    let trimmed = text.trim().strip_prefix("& ").unwrap_or(text.trim()).trim();
    if trimmed.starts_with("data:") && trimmed.contains(";base64,") {
        return Ok(Paste::Files(vec![Attachment {
            label: "Inline attachment".into(),
            payload: Payload::Source(trimmed.to_owned()),
        }]));
    }
    // Inspect only a small encoded prefix using the same signature registry. Arbitrary prose
    // and source code remain text; a long alphanumeric string is not assumed to be an image.
    if let Some(format) = encoded_format(trimmed) {
        let encoded_limit = settings.limits.max_input_bytes().saturating_add(2) / 3 * 4;
        if trimmed.len() as u64 > encoded_limit {
            return Err(eyre!(
                "Encoded attachment exceeds the configured media input limit"
            ));
        }
        let data: String = trimmed
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        return Ok(Paste::Files(vec![Attachment {
            label: format!("Pasted {}", format.mime_type),
            payload: Payload::Source(format!("data:{};base64,{data}", format.mime_type)),
        }]));
    }
    let unquoted = trimmed.trim_matches(['\'', '"']);
    if Path::new(unquoted).is_file() || root.join(unquoted).is_file() {
        return files_from_paths(vec![PathBuf::from(unquoted)], root);
    }
    let mut paths = Vec::new();
    let mut remaining = trimmed;
    while !remaining.is_empty() {
        let (part, rest) =
            if let Some(quote) = remaining.chars().next().filter(|c| matches!(c, '\'' | '"')) {
                let Some(end) = remaining[1..].find(quote) else {
                    return Ok(Paste::Text(text));
                };
                (&remaining[1..end + 1], &remaining[end + 2..])
            } else {
                let end = remaining
                    .find(char::is_whitespace)
                    .unwrap_or(remaining.len());
                (&remaining[..end], &remaining[end..])
            };
        let path = PathBuf::from(part);
        if !path.is_absolute() {
            return Ok(Paste::Text(text));
        }
        paths.push(path);
        remaining = rest.trim_start();
    }
    if paths.is_empty() {
        Ok(Paste::Text(text))
    } else {
        files_from_paths(paths, root)
    }
}

fn files_from_paths(paths: Vec<PathBuf>, root: &Path) -> Result<Paste> {
    paths
        .into_iter()
        .map(|path| {
            let path = cluaiz_api::cep::roots::resolve(&path.to_string_lossy(), root, false)
                .map_err(|e| eyre!("{e:?}"))?;
            if !path.is_file() {
                return Err(eyre!("Attach a file, not a directory: {}", path.display()));
            }
            let label = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            Ok(Attachment {
                label,
                payload: Payload::Source(path.to_string_lossy().into_owned()),
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(Paste::Files)
}

pub async fn prepare(
    input: Vec<Attachment>,
    tools: &mut Tools,
    settings: MediaSettings,
    history: &[ChatMessage<Vec<PreparedPart>>],
    model: &Path,
) -> Result<Vec<PreparedPart>> {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut guard = cluaiz_media::request::PreparationGuard::new(cancel.clone());
    let package = cluaiz_models::capabilities::PackageCapabilities::inspect(model)
        .map_err(|e| eyre!("{e:#}"))?;
    let resolver = SourceResolver::from_environment(settings)
        .map_err(|e| eyre!("{e:#}"))?
        .with_document_images(
            package
                .supported_inputs()
                .contains(&cluaiz_core::Modality::Image),
        )
        .with_cancellation(cancel.clone());
    let mut retained = PreparedPart::request_bytes(history).map_err(|error| eyre!("{error:#}"))?;
    let supported = package.supported_inputs();
    let mut result = Vec::new();
    for attachment in input {
        let parts = match attachment.payload {
            Payload::Prepared(parts) => parts,
            Payload::Source(source) => {
                let approved = if source.starts_with("data:") {
                    None
                } else {
                    Some(tools.attachment_path(&source).map_err(|e| eyre!(e))?)
                };
                let preparation = cluaiz_media::request::prepare_attachment(
                    &source,
                    &resolver,
                    retained,
                    &supported,
                    move |path| {
                        if approved.as_deref() == Some(path) {
                            Ok(())
                        } else {
                            anyhow::bail!("Attachment path changed after approval")
                        }
                    },
                );
                tokio::pin!(preparation);
                let mut tick = tokio::time::interval(crate::ui::spinner::TICK_INTERVAL);
                loop {
                    tokio::select! {
                        parts = &mut preparation => break parts.map_err(|e| eyre!("{}: {e:#}", attachment.label))?,
                        _ = tick.tick() => if cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst) { cancel.store(true, Ordering::Release); return Err(eyre!("Attachment preparation cancelled")); }
                    }
                }
            }
        };
        for part in parts {
            retained = retained
                .checked_add(u64::try_from(part.retained_bytes())?)
                .ok_or_else(|| eyre!("Attachment byte count overflow"))?;
            settings.limits.check_decoded_bytes(retained)?;
            if !supported.contains(&part.modality()) {
                return Err(eyre!(
                    "Selected model does not support {} input ({})",
                    match part {
                        PreparedPart::Image { .. } => "image",
                        PreparedPart::Audio { .. } => "audio",
                        _ => "this",
                    },
                    attachment.label
                ));
            }
            result.push(part);
        }
    }
    guard.complete();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_stays_text_and_quoted_file_drops_keep_spaces_and_order() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!candidate("What does this function do?"));
        let Paste::Text(text) = ingest(
            Ingress::Text("normal text".into()),
            dir.path(),
            MediaSettings::default(),
        )
        .unwrap() else {
            panic!("text became a file");
        };
        assert_eq!(text, "normal text");
        let first = dir.path().join("first note.md");
        let second = dir.path().join("second.json");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "{}").unwrap();
        let drop = format!("\"{}\" \"{}\"", first.display(), second.display());
        assert!(candidate(&drop));
        let Paste::Files(files) =
            ingest(Ingress::Text(drop), dir.path(), MediaSettings::default()).unwrap()
        else {
            panic!("file drop became text");
        };
        assert_eq!(
            files
                .iter()
                .map(|file| file.label.as_str())
                .collect::<Vec<_>>(),
            ["first note.md", "second.json"]
        );
        assert!(ingest(
            Ingress::Text(dir.path().display().to_string()),
            dir.path(),
            MediaSettings::default()
        )
        .is_err());
    }
    #[test]
    fn encoded_image_becomes_an_attachment_without_leaking_into_the_draft() {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10];
        bytes.resize(64, 0);
        let encoded = STANDARD.encode(bytes);
        assert!(candidate(&encoded));
        let Paste::Files(files) = ingest(
            Ingress::Text(encoded),
            Path::new("."),
            MediaSettings::default(),
        )
        .unwrap() else {
            panic!("encoded image became prose");
        };
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].label, "Pasted image/png");
        assert!(
            matches!(&files[0].payload, Payload::Source(source) if source.starts_with("data:image/png;base64,"))
        );
        assert!(!candidate(&"plain_alphanumeric_text".repeat(20)));
    }
}
