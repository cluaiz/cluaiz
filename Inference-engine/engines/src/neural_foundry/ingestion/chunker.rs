use std::cmp;

/// Intelligent Semantic Chunker for cluaiz OS
/// Parses developer documentation and codebases contextually without relying on external AST or Regex bloat.
pub struct SemanticChunker;

impl SemanticChunker {
    pub fn chunk(text: &str, extension: &str, max_chunk_size: usize) -> Vec<String> {
        match extension.to_lowercase().as_str() {
            "md" | "mdx" => Self::chunk_markdown(text, max_chunk_size),
            "rs" | "py" | "ts" | "js" | "c" | "cpp" | "go" => Self::chunk_code(text, max_chunk_size),
            _ => Self::chunk_fallback(text, max_chunk_size),
        }
    }

    /// Splits Markdown based on headers (`#`) ensuring semantic context remains intact.
    fn chunk_markdown(text: &str, max_size: usize) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for line in text.lines() {
            // Split at Headers if the current chunk isn't empty
            if line.trim_start().starts_with('#') && !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
            }
            
            // Hard split if a single section exceeds max_size
            if current_chunk.len() + line.len() > max_size {
                chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
            }

            current_chunk.push_str(line);
            current_chunk.push('\n');
        }

        if !current_chunk.trim().is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }

        chunks
    }

    /// Splits Code files based on empty lines (function boundaries) and hard limits.
    fn chunk_code(text: &str, max_size: usize) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for line in text.lines() {
            // Function boundary heuristic: Empty line when the chunk is decently sized
            if line.trim().is_empty() && current_chunk.len() > max_size / 2 {
                chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
                continue;
            }

            if current_chunk.len() + line.len() > max_size {
                chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
            }

            current_chunk.push_str(line);
            current_chunk.push('\n');
        }

        if !current_chunk.trim().is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }

        chunks
    }

    /// Fallback overlapping chunker for generic text (e.g. TXT, CSV)
    fn chunk_fallback(text: &str, max_size: usize) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut chunks = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            let end = cmp::min(i + max_size, chars.len());
            let chunk: String = chars[i..end].iter().collect();
            chunks.push(chunk.trim().to_string());
            i += max_size.saturating_sub(50).max(1); // 50 char overlap
        }

        if chunks.is_empty() && !text.trim().is_empty() {
            chunks.push(text.trim().to_string());
        }
        chunks
    }
}
