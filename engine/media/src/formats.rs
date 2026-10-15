//! Extensible content detection. A detected format does not imply an installed decoder/model.

use anyhow::{bail, Result};

const GENERIC_MIME: &str = "application/octet-stream";
const UNKNOWN_EXTENSION: &str = "bin";
const OPENXML_MIME_PREFIX: &str = "application/vnd.openxmlformats-officedocument.";
const OPENDOCUMENT_MIME_PREFIX: &str = "application/vnd.oasis.opendocument.";

/// Resolve inline-audio transport hints using the existing MIME registry.
pub fn audio_mime(extension: &str) -> Result<&'static str> {
    if !extension
        .bytes()
        .all(|character| character.is_ascii_alphanumeric())
    {
        bail!("Invalid inline audio format");
    }
    mime_guess::from_ext(extension)
        .iter_raw()
        .find(|mime| mime.starts_with("audio/"))
        .ok_or_else(|| anyhow::anyhow!("Unknown inline audio format"))
}

/// Codec-library descriptors supply extensions; container support still requires
/// a decodable audio track and does not imply a native video input provider.
pub fn attachment_formats() -> Vec<(cluaiz_core::Modality, &'static str)> {
    use cluaiz_core::Modality;
    use symphonia::{core::probe::QueryDescriptor, default::formats};
    let mut formats: Vec<_> = image::ImageFormat::all()
        // The byte-only decoder uses guess_format, which cannot identify TGA.
        // can_read alone also includes codecs disabled in this compiled image library.
        .filter(|format| {
            format.can_read() && format.reading_enabled() && *format != image::ImageFormat::Tga
        })
        .flat_map(|format| {
            format
                .extensions_str()
                .iter()
                .map(|extension| (Modality::Image, *extension))
        })
        .collect();
    for descriptor in [
        formats::AdtsReader::query(),
        formats::FlacReader::query(),
        formats::IsoMp4Reader::query(),
        formats::MpaReader::query(),
        formats::WavReader::query(),
        formats::OggReader::query(),
        formats::MkvReader::query(),
    ]
    .into_iter()
    .flatten()
    {
        formats.extend(
            descriptor
                .extensions
                .iter()
                .filter(|extension| {
                    mime_guess::from_ext(extension)
                        .first_raw()
                        .is_some_and(|mime| mime.starts_with("audio/"))
                })
                .map(|extension| (Modality::Audio, *extension)),
        );
    }
    formats
}

/// One decoder-provided extension per MIME type, instead of every spelling/alias.
/// This only normalizes supplied descriptors; it does not invent decoder support.
pub fn canonical_extensions<'a>(extensions: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = std::collections::BTreeSet::new();
    extensions
        .into_iter()
        .filter(|extension| {
            let key = match mime_guess::from_ext(extension).first_raw() {
                Some(mime) if mime != GENERIC_MIME => mime,
                _ => *extension,
            };
            seen.insert(key)
        })
        .collect()
}

/// Source format evidence, distinct from model capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFormat {
    pub mime_type: String,
    pub extension: String,
    pub content_detected: bool,
}

/// Document providers registered at the format boundary, separate from inference capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Text,
    Pdf,
    Docx,
    Image,
}

/// Positive registered document providers. PDF/DOCX support means text extraction,
/// with visual/empty pages reported separately; it is not native model file input.
pub fn document_attachment_formats() -> Vec<(cluaiz_core::Modality, &'static str)> {
    [
        "application/pdf",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "text/plain",
        "text/markdown",
        "application/json",
        "text/csv",
    ]
    .into_iter()
    .flat_map(|mime| mime_guess::get_mime_extensions_str(mime).unwrap_or_default())
    .map(|extension| (cluaiz_core::Modality::Text, *extension))
    .collect()
}

impl MediaFormat {
    pub fn document_kind(&self, bytes: &[u8]) -> Result<DocumentKind> {
        match self.mime_type.as_str() {
            mime if mime.starts_with("image/") => Ok(DocumentKind::Image),
            "application/pdf" => Ok(DocumentKind::Pdf),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Ok(DocumentKind::Docx)
            }
            "application/zip" => {
                let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
                archive.by_name("word/document.xml")?;
                Ok(DocumentKind::Docx)
            }
            mime if mime.starts_with("text/")
                || mime == GENERIC_MIME
                || !self.content_detected
                || matches!(
                    mime,
                    "application/json"
                        | "application/xml"
                        | "application/yaml"
                        | "application/javascript"
                ) =>
            {
                // A suffix/MIME hint alone cannot make actual UTF-8 code into video
                // (for example a transport-stream suffix shared by a source-code format).
                let text = std::str::from_utf8(bytes)?;
                if text
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                {
                    bail!("Binary/control data cannot be treated as document text");
                }
                Ok(DocumentKind::Text)
            }
            _ => bail!("No document extractor is registered for {}", self.mime_type),
        }
    }
}

/// Library-provided signature matchers plus caller-registered custom matchers.
#[derive(Default)]
pub struct FormatRegistry {
    detector: infer::Infer,
}

impl FormatRegistry {
    /// Registers a signature matcher without adding extension branches to compute code.
    pub fn register(
        &mut self,
        mime: &'static str,
        extension: &'static str,
        matcher: infer::Matcher,
    ) -> Result<()> {
        if extension.is_empty() || !extension.bytes().all(|c| c.is_ascii_alphanumeric()) {
            bail!("Format extension must be a non-empty ASCII identifier");
        }
        self.detector.add(mime, extension, matcher);
        Ok(())
    }

    /// Prefers signature evidence; rejects a contradictory specific MIME hint.
    pub fn detect(&self, bytes: &[u8], mime_hint: Option<&str>) -> Result<MediaFormat> {
        let hint = mime_hint
            .and_then(|h| h.split(';').next())
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .map(str::to_ascii_lowercase);
        if let Some(found) = self.detector.get(bytes) {
            if let Some(ref hint) = hint {
                let aliases = mime_guess::get_mime_extensions_str(hint).unwrap_or_default();
                let is_container_hint = found.mime_type() == "application/zip"
                    && (hint.starts_with(OPENXML_MIME_PREFIX)
                        || hint.starts_with(OPENDOCUMENT_MIME_PREFIX));
                if hint != GENERIC_MIME
                    && hint != found.mime_type()
                    && !aliases.contains(&found.extension())
                    && !is_container_hint
                {
                    bail!(
                        "Declared MIME '{}' contradicts detected '{}'",
                        hint,
                        found.mime_type()
                    );
                }
                if is_container_hint {
                    return Ok(Self::hint_format(hint.clone()));
                }
            }
            return Ok(MediaFormat {
                mime_type: found.mime_type().to_owned(),
                extension: found.extension().to_owned(),
                content_detected: true,
            });
        }
        Ok(Self::hint_format(
            hint.unwrap_or_else(|| GENERIC_MIME.to_owned()),
        ))
    }

    fn hint_format(mime_type: String) -> MediaFormat {
        let extension = mime_guess::get_mime_extensions_str(&mime_type)
            .and_then(|extensions| extensions.first())
            .copied()
            .unwrap_or(UNKNOWN_EXTENSION)
            .to_owned();
        MediaFormat {
            mime_type,
            extension,
            content_detected: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formats_signature_wins_and_contradictory_hint_rejects() {
        let registry = FormatRegistry::default();
        let jpeg_header = [0xff, 0xd8, 0xff, 0xaa];
        assert_eq!(
            registry.detect(&jpeg_header, None).unwrap().mime_type,
            "image/jpeg"
        );
        assert!(registry.detect(&jpeg_header, Some("image/png")).is_err());
        assert!(
            registry
                .detect(&jpeg_header, Some(GENERIC_MIME))
                .unwrap()
                .content_detected
        );
    }

    #[test]
    fn test_formats_custom_matcher_is_reusable_and_path_suffix_rejects() {
        let mut registry = FormatRegistry::default();
        registry
            .register("test/custom", "custom", |bytes| {
                bytes.starts_with(b"CUSTOM")
            })
            .unwrap();
        assert_eq!(
            registry.detect(b"CUSTOM data", None).unwrap().extension,
            "custom"
        );
        assert!(registry
            .register("test/invalid", "../escape", |_| true)
            .is_err());
        assert!(
            !registry
                .detect(b"plain text", Some("text/plain"))
                .unwrap()
                .content_detected
        );
    }
}
