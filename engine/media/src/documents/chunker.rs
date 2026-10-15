//! Paragraph-aware chunking with a hard UTF-8 byte ceiling, independent of file extensions.

use anyhow::{bail, Result};

pub struct SemanticChunker;

impl SemanticChunker {
    /// Retains document order; long paragraphs/lines also obey the byte ceiling.
    pub fn chunk(text: &str, max_bytes: usize) -> Result<Vec<String>> {
        if max_bytes == 0 {
            bail!("Chunk byte size must be positive");
        }
        let mut chunks = Vec::new();
        let mut current = String::new();
        for line in text.split_inclusive('\n') {
            if (line.trim().is_empty() || line.trim_start().starts_with('#')) && !current.is_empty()
            {
                emit(&mut chunks, &mut current);
            }
            for character in line.chars() {
                if character.len_utf8() > max_bytes {
                    bail!("Chunk limit cannot hold one UTF-8 character");
                }
                if current.len() + character.len_utf8() > max_bytes {
                    emit(&mut chunks, &mut current);
                }
                current.push(character);
            }
        }
        emit(&mut chunks, &mut current);
        Ok(chunks)
    }
}

fn emit(chunks: &mut Vec<String>, current: &mut String) {
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_owned());
    }
    current.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_chunks_long_unicode_lines_respect_bytes_and_order() {
        let text = "हैलो世界".repeat(9);
        let chunks = SemanticChunker::chunk(&text, 13).unwrap();
        assert!(chunks
            .iter()
            .all(|chunk| !chunk.is_empty() && chunk.len() <= 13));
        assert_eq!(chunks.concat(), text);
        assert!(SemanticChunker::chunk("x", 0).is_err());
        assert!(SemanticChunker::chunk("🙂", 1).is_err());
    }
}
