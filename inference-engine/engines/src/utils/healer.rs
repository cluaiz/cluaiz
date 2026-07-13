//! Healer module for automatically resolving common engine issues like missing tokenizers.
use std::path::Path;

pub struct AutoHealer;

impl AutoHealer {
    /// Checks if a tokenizer exists for the given repo; if not, triggers an auto-download.
    pub async fn heal_missing_tokenizer(_repo_id: &str, _dest_dir: &Path) -> Result<(), String> {
        // Tokenizer healing no longer required as native engines read vocab from GGUF headers.
        Ok(())
    }
}
