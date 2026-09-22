use color_eyre::Result;

/// 🏛️ The Universal Skill Injector Trait
/// Any skill (Markdown, JSON MCP, OpenClaw, KV-Cache) is mapped into this trait.
pub trait NativeSkillInjector {
    /// Tokenizes text for subword injection (parse_special = false)
    fn get_injection_subwords(&self) -> Result<Vec<String>>;

    /// Returns memory mapped pointers if this skill has a precomputed .kv-cache state
    fn get_tensor_graft_pointers(&self) -> Option<Vec<f32>>;
    
    /// Manipulate logits right before sampling
    fn steer_logit_probabilities(&self, logits: &mut [f32]);
}

/// A standard skill loaded from .md format
pub struct MarkdownSkill {
    pub name: String,
    pub raw_content: String,
}

impl NativeSkillInjector for MarkdownSkill {
    fn get_injection_subwords(&self) -> Result<Vec<String>> {
        // Fallback for Phase 2 implementation. The tokenization itself will happen
        // inside `stream.rs` using the native llama_tokenize engine, but this prepares
        // the raw string block.
        Ok(vec![self.raw_content.clone()])
    }

    fn get_tensor_graft_pointers(&self) -> Option<Vec<f32>> {
        // Markdown skills have no precomputed binary tensors.
        None
    }

    fn steer_logit_probabilities(&self, _logits: &mut [f32]) {
        // Text-based skills do not inherently bias logits.
    }
}
