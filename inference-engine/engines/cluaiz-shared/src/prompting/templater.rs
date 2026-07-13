use std::collections::HashMap;
use minijinja::{Environment, context};
use tracing::warn;

/// 🎭 TemplateManager: Handles neural prompt formatting based on model DNA.
#[derive(Debug, Clone)]
pub struct TemplateManager {
    pub templates: HashMap<String, String>,
}

// Default fallback templates if discovery fails (universal baselines)
const FALLBACK_CHATML: &str = "<|im_start|>user\n{{prompt}}<|im_end|>\n<|im_start|>assistant\n";
const FALLBACK_LLAMA3: &str = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\n{{prompt}}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";

impl Default for TemplateManager {
    fn default() -> Self {
        Self { templates: HashMap::new() }
    }
}

impl TemplateManager {
    pub fn format(&self, dna: &crate::metadata::dna::StructuralDNA, prompt: &str) -> String {
        // 1. Priority: Use discovered template from DNA (JSON driven)
        if let Some(ref template) = dna.chat_template {
            let mut env = Environment::new();
            
            // Try to parse via MiniJinja natively
            if let Ok(_) = env.add_template("chat", template) {
                // Construct standard messages array for single-turn prompt
                let messages = vec![
                    context! { role => "user", content => prompt }
                ];
                let ctx = context! { messages => messages, add_generation_prompt => true };
                
                if let Ok(tmpl) = env.get_template("chat") {
                    if let Ok(rendered) = tmpl.render(ctx) {
                        return rendered;
                    } else {
                        warn!("⚠️ [Templater] MiniJinja failed to render template. Falling back to simple replacement.");
                    }
                }
            } else {
                warn!("⚠️ [Templater] MiniJinja failed to parse the template syntax. Using raw fallback.");
            }

            // Simple replace as emergency fallback for strict formats if minijinja fails
            if template.contains("<|im_start|>") {
                return FALLBACK_CHATML.replace("{{prompt}}", prompt);
            }
            if template.contains("<|start_header_id|>") {
                return FALLBACK_LLAMA3.replace("{{prompt}}", prompt);
            }
            if template.contains("<start_of_turn>") {
                return format!("<start_of_turn>user\n{}<end_of_turn>\n<start_of_turn>model\n", prompt);
            }
        }

        // 2. Secondary: Guess by architecture name
        let arch = dna.model_identity.to_lowercase();
        let final_template = if arch.contains("llama") {
            FALLBACK_LLAMA3
        } else if arch.contains("gemma") {
            "<start_of_turn>user\n{{prompt}}<end_of_turn>\n<start_of_turn>model\n"
        } else {
            FALLBACK_CHATML // Qwen / ChatML default
        };

        final_template.replace("{{prompt}}", prompt)
    }

    /// Forms a strict mid-conversation turn for Pivot/Interrupt scenarios.
    /// This ensures we close the current assistant turn and start a proper user turn.
    pub fn format_turn(&self, dna: &crate::metadata::dna::StructuralDNA, prompt: &str) -> String {
        let arch = dna.model_identity.to_lowercase();
        if arch.contains("llama") {
            format!("<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n", prompt)
        } else if arch.contains("gemma") {
            format!("<end_of_turn>\n<start_of_turn>user\n{}<end_of_turn>\n<start_of_turn>model\n", prompt)
        } else {
            // Qwen / ChatML default
            format!("<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n", prompt)
        }
    }
}
