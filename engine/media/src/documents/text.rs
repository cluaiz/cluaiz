//! Content-based text/code reader; no source-language or extension-specific branches.

use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;

pub(super) fn decode(bytes: &[u8], limits: MediaLimits) -> Result<(String, &'static str)> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little_endian = bytes[0] == 0xff;
        let payload = &bytes[2..];
        if !payload.len().is_multiple_of(2) {
            bail!("Document UTF-16 has an incomplete code unit");
        }
        let units = || {
            payload.chunks_exact(2).map(|pair| {
                let pair = [pair[0], pair[1]];
                if little_endian {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                }
            })
        };
        // Count before allocating; a second linear pass needs no intermediate Vec<u16>.
        let mut size = 0usize;
        for character in char::decode_utf16(units()) {
            let character = character.context("Document UTF-16 contains an invalid surrogate")?;
            validate_character(character)?;
            size = size
                .checked_add(character.len_utf8())
                .context("Text size overflow")?;
            limits.check_decoded_bytes(u64::try_from(size)?)?;
        }
        let mut text = String::new();
        text.try_reserve_exact(size)?;
        for character in char::decode_utf16(units()) {
            text.push(character.context("Invalid UTF-16 document")?);
        }
        require_content(&text)?;
        return Ok((text, if little_endian { "utf16le" } else { "utf16be" }));
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    limits.check_decoded_bytes(u64::try_from(bytes.len())?)?;
    let text =
        std::str::from_utf8(bytes).context("Document must be UTF-8 or BOM-marked UTF-16 text")?;
    for character in text.chars() {
        validate_character(character)?;
    }
    require_content(text)?;
    let mut owned = String::new();
    owned.try_reserve_exact(text.len())?;
    owned.push_str(text);
    Ok((owned, "utf8"))
}

fn validate_character(character: char) -> Result<()> {
    if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
        bail!("Binary/control data cannot be treated as document text");
    }
    Ok(())
}

fn require_content(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        bail!("Document text is empty");
    }
    Ok(())
}
