/// Intelligent Semantic Chunker for Cluaiz
/// Parses developer documentation and codebases contextually without relying on external AST or Regex bloat.
pub struct SemanticChunker;

impl SemanticChunker {
    pub fn chunk(text: &str, extension: &str, max_chunk_size: usize) -> Vec<String> {
        let ext = extension.to_lowercase();
        if Self::is_markdown(&ext) {
            Self::chunk_markdown(text, max_chunk_size)
        } else if Self::is_code(&ext) {
            Self::chunk_code(text, max_chunk_size)
        } else {
            Self::chunk_fallback(text, max_chunk_size)
        }
    }

    /// Check if extension corresponds to markdown/rich document formatting
    pub fn is_markdown(ext: &str) -> bool {
        matches!(ext, "md" | "mdx" | "markdown" | "mdown" | "rst")
    }

    /// Check if extension corresponds to source code
    pub fn is_code(ext: &str) -> bool {
        matches!(
            ext,
            "rs" | "py" | "ts" | "js" | "tsx" | "jsx" | "c" | "cpp" | "h" | "hpp" | "cc" |
            "go" | "java" | "kt" | "kts" | "cs" | "swift" | "rb" | "php" | "scala" | "sh" |
            "bash" | "zsh" | "lua" | "r" | "dart" | "zig" | "sql" | "json" | "yaml" | "yml" |
            "toml" | "xml" | "html" | "css" | "scss" | "proto"
        )
    }

    /// Splits Markdown based on headers (`#`) ensuring semantic context remains intact.
    fn chunk_markdown(text: &str, max_size: usize) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for line in text.lines() {
            if line.trim_start().starts_with('#') && !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
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

    /// Splits Code files based on empty lines (function boundaries) and hard limits.
    fn chunk_code(text: &str, max_size: usize) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for line in text.lines() {
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

    /// Fallback strategy for general plain text (splits by paragraphs)
    fn chunk_fallback(text: &str, max_size: usize) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for paragraph in text.split("\n\n") {
            if current_chunk.len() + paragraph.len() > max_size {
                if !current_chunk.is_empty() {
                    chunks.push(current_chunk.trim().to_string());
                    current_chunk.clear();
                }
            }
            
            if paragraph.len() > max_size {
                for line in paragraph.lines() {
                    if current_chunk.len() + line.len() > max_size && !current_chunk.is_empty() {
                        chunks.push(current_chunk.trim().to_string());
                        current_chunk.clear();
                    }
                    current_chunk.push_str(line);
                    current_chunk.push('\n');
                }
            } else {
                current_chunk.push_str(paragraph);
                current_chunk.push_str("\n\n");
            }
        }

        if !current_chunk.trim().is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }

        chunks
    }
}
