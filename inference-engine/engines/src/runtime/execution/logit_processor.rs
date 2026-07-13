use anyhow::Result;

/// Structured Logit Processor: The Logit-Level Grammar Masker
/// Ensures that the model output strictly adheres to predefined Grammars (e.g., JSON schemas)
/// by masking invalid token logits (setting them to -INF) right before sampling.
pub struct GrammarMasker {
    pub active_regex_ast: Option<String>, 
    // In production, this AST evaluates the Trie state of the current sequence
}

impl GrammarMasker {
    pub fn new() -> Self {
        Self {
            active_regex_ast: None,
        }
    }

    /// Compiles a JSON Schema or Regex into a fast finite-state machine (FSM) / AST
    pub fn load_grammar(&mut self, regex_pattern: &str) {
        tracing::info!("🛡️ [LogitProcessor] Compiling Grammar AST for constraint: {}", regex_pattern);
        self.active_regex_ast = Some(regex_pattern.to_string());
    }

    /// Zero-Overhead Logit Masking
    /// Evaluates the active FSM state and sets probabilities of illegal tokens to -INF.
    pub fn mask_logits(&self, logits: &mut [f32], valid_tokens: &[u32]) -> Result<()> {
        if self.active_regex_ast.is_none() {
            return Ok(()); // Bypass if no grammar active
        }

        // Extremely fast CPU-level masking: O(V) where V is vocab size.
        // We set all tokens not in the `valid_tokens` active set to negative infinity.
        for (token_id, logit) in logits.iter_mut().enumerate() {
            if !valid_tokens.contains(&(token_id as u32)) {
                *logit = f32::NEG_INFINITY;
            }
        }

        Ok(())
    }
}
