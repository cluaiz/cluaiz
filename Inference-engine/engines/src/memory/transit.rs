use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{info, warn};
use super::embedding_generator::EmbeddingGenerator;
use super::storage_bridge::load_storage_bridge;

/// Configuration for the Transit Lounge (Ring Buffer)
pub struct TransitConfig {
    pub auto_flush: bool,         // True = Automatic mode, False = Custom
    pub flush_threshold: usize,   // Used if auto_flush is false (e.g., tokens or sentences)
    pub memory_limit_mb: usize,   // Maximum RAM to use before forcing a flush
}

impl Default for TransitConfig {
    fn default() -> Self {
        Self {
            auto_flush: true,
            flush_threshold: 100, // Flush after 100 tokens/sentences in Custom mode
            memory_limit_mb: 256, // Max 256MB RAM before forced flush
        }
    }
}

/// A single unit of unconfirmed execution or generation output
#[derive(Debug, Clone)]
pub struct TransitToken {
    pub session_id: String,
    pub text: String,
    pub is_boundary: bool, // True if this token is a sentence boundary (e.g. '.', '\n', space depending on rules)
}

/// Lock-free Transit Lounge (Ring Buffer)
/// Holds context strictly in RAM to prevent disk trashing, committing to SSD in batches.
pub struct TransitLounge {
    config: TransitConfig,
    token_count: Arc<AtomicUsize>,
    tx: mpsc::Sender<TransitToken>,
    buffer: Mutex<Vec<TransitToken>>,
}

impl TransitLounge {
    pub fn new(config: TransitConfig, tx: mpsc::Sender<TransitToken>) -> Self {
        info!("🚄 [TransitLounge] Initializing RAM Ring Buffer (Auto Flush: {})", config.auto_flush);
        Self {
            config,
            token_count: Arc::new(AtomicUsize::new(0)),
            tx,
            buffer: Mutex::new(Vec::new()),
        }
    }

    /// Push a token to the lock-free RAM ring buffer.
    pub async fn push_token(&self, token: TransitToken) -> anyhow::Result<()> {
        let current_count = self.token_count.fetch_add(1, Ordering::Relaxed);
        
        // Push a clone of the token into the buffer before transmitting it.
        {
            let mut buf = self.buffer.lock().unwrap();
            buf.push(token.clone());
        }

        // 1. Check for Hybrid Control Flush Triggers
        let mut should_flush = false;

        if self.config.auto_flush {
            // Option A: Automatic Kernel Limit
            // Simulated check: If we hit a conceptual boundary, or VRAM pressure is high.
            if token.is_boundary {
                should_flush = true;
            }
        } else {
            // Option B: Custom Limits
            if current_count > 0 && current_count % self.config.flush_threshold == 0 {
                should_flush = true;
            }
        }

        self.tx.send(token).await?;

        if should_flush {
            self.trigger_batch_commit().await;
        }

        Ok(())
    }

    /// Triggers the background SSD sync
    async fn trigger_batch_commit(&self) {
        info!("💾 [TransitLounge] Sentence Boundary / Threshold Reached. Triggering SSD Batch Flush...");
        
        // Drain the tokens buffer
        let tokens = {
            let mut buf = self.buffer.lock().unwrap();
            std::mem::take(&mut *buf)
        };

        if tokens.is_empty() {
            return;
        }

        // We assume all tokens in this batch belong to the same session/session_id
        let session_id = tokens[0].session_id.clone();

        // Concatenate the tokens' text fields
        let combined_text: String = tokens.into_iter().map(|t| t.text).collect();

        // Generate a dynamic embedding vector
        let vector = EmbeddingGenerator::generate_vector(&combined_text);

        // Instantiate the storage bridge and save context to LMDB/Remote
        let bridge = load_storage_bridge();
        if let Err(e) = bridge.save_context(&session_id, &combined_text, &vector) {
            warn!("❌ [TransitLounge] Failed to commit batch context to storage: {}", e);
        } else {
            info!("✅ [TransitLounge] Successfully committed batch context ({} bytes) for session '{}'", combined_text.len(), session_id);
        }
    }
}
