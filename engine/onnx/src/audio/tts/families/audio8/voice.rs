//! Adapted from Edge0-AI/Audio8_TTS@07e40f5 (Apache-2.0); modified native Rust runtime.
//! Registered upstream voices: reference_text plus C-order uint16/int64 codec matrices.
use super::package::{asset, Manifest};
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use std::path::{Component, Path};

pub(super) struct Reference {
    pub text: String,
    pub codes: Vec<i64>,
    pub frames: usize,
}

pub(super) fn load(dir: &Path, name: &str, m: &Manifest, limits: MediaLimits) -> Result<Reference> {
    if name.is_empty()
        || name.contains(['/', '\\', ':'])
        || Path::new(name).components().count() != 1
        || !matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
    {
        bail!("Invalid Audio8 voice id");
    }
    let meta = asset(dir, &format!("voices/{name}/meta.json"))?;
    let codes = asset(dir, &format!("voices/{name}/codes.npy"))?;
    limits.check_input_bytes(std::fs::metadata(&meta)?.len())?;
    limits.check_input_bytes(std::fs::metadata(&codes)?.len())?;
    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(meta)?)?;
    if metadata["model_fingerprint"].as_str() != Some(m.model_fingerprint.as_str()) {
        bail!("Audio8 reference voice fingerprint does not match the selected model package");
    }
    let text = metadata["reference_text"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .context("Audio8 voice requires reference_text")?
        .to_owned();
    let bytes = std::fs::read(codes)?;
    let (codes, frames) = matrix(&bytes, m.num_codebooks, m.codebook_size, limits)?;
    Ok(Reference {
        text,
        codes,
        frames,
    })
}

/// Restricted NPY v1/v2/v3 reader; no pickle/object payload or executable header evaluation.
fn matrix(
    bytes: &[u8],
    books: usize,
    codebook_size: usize,
    limits: MediaLimits,
) -> Result<(Vec<i64>, usize)> {
    if bytes.len() < 10 || &bytes[..6] != b"\x93NUMPY" || bytes[7] != 0 {
        bail!("Invalid Audio8 voice NPY signature");
    }
    let (offset, length) = match bytes[6] {
        1 => (
            10usize,
            usize::from(u16::from_le_bytes(bytes[8..10].try_into()?)),
        ),
        2 | 3 if bytes.len() >= 12 => (
            12usize,
            usize::try_from(u32::from_le_bytes(bytes[8..12].try_into()?))?,
        ),
        _ => bail!("Unsupported Audio8 voice NPY version"),
    };
    let end = offset
        .checked_add(length)
        .filter(|n| *n <= bytes.len())
        .context("Truncated voice NPY header")?;
    // NPY's three-field literal header contains only dtype, order and integer dimensions.
    // Convert this limited grammar to data JSON, never run Python or accept object arrays.
    let header = std::str::from_utf8(&bytes[offset..end])?
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>()
        .replace('\'', "\"")
        .replace("False", "false")
        .replace('(', "[")
        .replace(')', "]")
        .replace(",]", "]")
        .replace(",}", "}");
    let header: serde_json::Value = serde_json::from_str(&header)?;
    let shape = header["shape"]
        .as_array()
        .context("Missing voice NPY shape")?;
    let width = match header["descr"].as_str() {
        Some("<i8") => 8,
        Some("<u2") => 2,
        _ => bail!("Audio8 voice codes must be little-endian uint16 or int64"),
    };
    if header["fortran_order"] != false
        || shape.len() != 2
        || shape[0].as_u64() != Some(u64::try_from(books)?)
    {
        bail!("Audio8 voice requires a C-order [num_codebooks,T] matrix");
    }
    let frames = shape[1]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n > 0)
        .context("Empty voice code matrix")?;
    let count = books
        .checked_mul(frames)
        .context("Voice matrix size overflow")?;
    let size = count
        .checked_mul(width)
        .context("Voice matrix byte overflow")?;
    limits.check_decoded_bytes(u64::try_from(
        count
            .checked_mul(8)
            .and_then(|n| n.checked_add(bytes.len()))
            .context("Voice matrix memory overflow")?,
    )?)?;
    if bytes.len() - end != size {
        bail!("Voice NPY payload length mismatch");
    }
    let codes: Vec<i64> = bytes[end..]
        .chunks_exact(width)
        .map(|b| {
            if width == 2 {
                Ok(i64::from(u16::from_le_bytes(b.try_into()?)))
            } else {
                Ok(i64::from_le_bytes(b.try_into()?))
            }
        })
        .collect::<Result<_>>()?;
    if codes
        .iter()
        .any(|&c| c < 0 || usize::try_from(c).map_or(true, |c| c >= codebook_size))
    {
        bail!("Audio8 voice contains out-of-range codec tokens");
    }
    Ok((codes, frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_voice_matrix_shape_range_and_truncation() -> Result<()> {
        let header = b"{'descr': '<i8', 'fortran_order': False, 'shape': (2, 2), }\n";
        let mut bytes = b"\x93NUMPY\x01\x00".to_vec();
        bytes.extend(u16::try_from(header.len())?.to_le_bytes());
        bytes.extend(header);
        for n in [1i64, 2, 3, 4] {
            bytes.extend(n.to_le_bytes());
        }
        assert_eq!(
            matrix(&bytes, 2, 4096, MediaLimits::default())?,
            (vec![1, 2, 3, 4], 2)
        );
        assert!(matrix(&bytes, 3, 4096, MediaLimits::default()).is_err());
        assert!(matrix(&bytes, 2, 4, MediaLimits::default()).is_err());
        assert!(matrix(&bytes[..bytes.len() - 1], 2, 4096, MediaLimits::default()).is_err());
        assert!(matrix(&bytes, 2, 4096, MediaLimits::new(1024, 8)?).is_err());
        let header = b"{'descr': '<u2', 'fortran_order': False, 'shape': (2, 2), }\n";
        let mut bytes = b"\x93NUMPY\x01\x00".to_vec();
        bytes.extend(u16::try_from(header.len())?.to_le_bytes());
        bytes.extend(header);
        for n in [1u16, 2, 3, 4] {
            bytes.extend(n.to_le_bytes());
        }
        assert_eq!(
            matrix(&bytes, 2, 4096, MediaLimits::default())?,
            (vec![1, 2, 3, 4], 2)
        );
        Ok(())
    }
}
