use std::collections::HashMap;
use minijinja::{Environment, context};
use tracing::warn;
use serde_json::json;

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
            
            // Add dummy functions that some models (like Llama 3) use in their jinja templates
            env.add_function("raise_exception", |err: String| -> Result<String, minijinja::Error> {
                Err(minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, err))
            });
            
            // Try to parse via MiniJinja natively
            if let Ok(_) = env.add_template("chat", template) {
                // Parse prompt as JSON array of messages (sent by chat API)
                let parsed_messages: Result<Vec<serde_json::Value>, _> = serde_json::from_str(prompt);
                
                let messages = if let Ok(msgs) = parsed_messages {
                    msgs
                } else {
                    // Fallback for single-turn strings (e.g. CLI or internal tests)
                    vec![json!({ "role": "user", "content": prompt })]
                };
                
                let ctx = context! { 
                    messages => messages, 
                    add_generation_prompt => true,
                    bos_token => "",
                    eos_token => ""
                };
                
                if let Ok(tmpl) = env.get_template("chat") {
                    if let Ok(rendered) = tmpl.render(ctx) {
                        return rendered;
                    } else {
                        warn!("⚠️ [Templater] MiniJinja failed to render template. Falling back to manual loop.");
                    }
                }
            } else {
                warn!("⚠️ [Templater] MiniJinja failed to parse the template syntax. Using manual fallback.");
            }

            // Simple replace as emergency fallback for strict formats if minijinja fails
            let mut is_llama3 = template.contains("<|start_header_id|>");
            let mut is_chatml = template.contains("<|im_start|>");
            let mut is_gemma = template.contains("<start_of_turn>");
            
            if is_llama3 || is_chatml || is_gemma {
                let mut fallback = String::new();
                if is_llama3 { fallback.push_str("<|begin_of_text|>"); }
                
                if let Ok(msgs) = serde_json::from_str::<Vec<serde_json::Value>>(prompt) {
                    for m in msgs {
                        if let (Some(role), Some(content)) = (m.get("role").and_then(|r| r.as_str()), m.get("content").and_then(|c| c.as_str())) {
                            if is_llama3 {
                                fallback.push_str(&format!("<|start_header_id|>{role}<|end_header_id|>\n\n{content}<|eot_id|>"));
                            } else if is_chatml {
                                fallback.push_str(&format!("<|im_start|>{role}\n{content}<|im_end|>\n"));
                            } else if is_gemma {
                                fallback.push_str(&format!("<start_of_turn>{role}\n{content}<end_of_turn>\n"));
                            }
                        }
                    }
                } else {
                    if is_llama3 {
                        fallback.push_str(&format!("<|start_header_id|>user<|end_header_id|>\n\n{prompt}<|eot_id|>"));
                    } else if is_chatml {
                        fallback.push_str(&format!("<|im_start|>user\n{prompt}<|im_end|>\n"));
                    } else if is_gemma {
                        fallback.push_str(&format!("<start_of_turn>user\n{prompt}<end_of_turn>\n"));
                    }
                }
                
                if is_llama3 {
                    fallback.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
                } else if is_chatml {
                    fallback.push_str("<|im_start|>assistant\n");
                } else if is_gemma {
                    fallback.push_str("<start_of_turn>model\n");
                }
                return fallback;
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
