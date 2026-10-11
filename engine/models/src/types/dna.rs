use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub struct KernelSignature {
    pub has_experts: bool,
    pub is_asymmetric: bool,
    pub is_multimodal: bool,
    pub is_heterogeneous: bool,
    pub is_bitnet: bool,
    pub is_ssm: bool,
    pub head_pattern: String,
    pub activation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralDNA {
    pub model_identity: String,
    pub layer_count: Option<usize>,
    pub attention_head_count: Option<usize>,
    pub attention_head_count_kv: Option<usize>,
    pub attention_head_dim: Option<usize>,
    pub hidden_size: Option<usize>,
    pub intermediate_size: Option<usize>,
    pub attention_dimensionality_truth: Option<usize>,
    pub signature: KernelSignature,
    pub max_context_length: Option<usize>,
    pub eos_token: Option<String>,
    pub chat_template: Option<String>,
    pub stop_sequences: Vec<String>,
    pub inference_params: HashMap<String, String>,
    pub dynamic_attributes: HashMap<String, String>,
    pub vram_headroom_gb: f32,
    pub ram_headroom_gb: f32,
    pub requires_gpu: bool,
    pub weights_size_gb: f32,
    pub weights_already_loaded: bool,
    pub guidance_bias: Option<HashMap<i32, f32>>,
    pub supports_thinking: bool,
    pub think_tag_schema: String,
    pub think_end_schema: String,
    pub reliable_think_close: bool,
}

impl Default for StructuralDNA {
    fn default() -> Self {
        Self {
            model_identity: "unknown".into(),
            layer_count: None,
            attention_head_count: None,
            attention_head_count_kv: None,
            attention_head_dim: None,
            hidden_size: None,
            intermediate_size: None,
            attention_dimensionality_truth: None,
            signature: KernelSignature::default(),
            max_context_length: None,
            eos_token: None,
            chat_template: None,
            stop_sequences: Vec::new(),
            inference_params: HashMap::new(),
            dynamic_attributes: HashMap::new(),
            vram_headroom_gb: 0.0,
            ram_headroom_gb: 0.0,
            requires_gpu: false,
            weights_size_gb: 0.0,
            weights_already_loaded: false,
            guidance_bias: None,
            supports_thinking: false,
            think_tag_schema: String::new(),
            think_end_schema: String::new(),
            reliable_think_close: false,
        }
    }
}

impl StructuralDNA {
    pub fn discover_from_path(&mut self, _model_dir: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn requires_fp16_kv(&self) -> bool {
        self.signature.is_ssm || self.attention_head_dim.unwrap_or(128) > 256
    }

    pub fn supports_flash_attention(&self) -> bool {
        !self.signature.is_ssm && self.attention_head_dim.unwrap_or(128) <= 256
    }

    pub fn parse_context_string(ctx_str: &str) -> usize {
        let normalized = ctx_str.to_lowercase();
        if normalized.ends_with('k') {
            let num = normalized
                .trim_end_matches('k')
                .parse::<usize>()
                .unwrap_or(4);
            num * 1024
        } else if normalized.ends_with('m') {
            let num = normalized
                .trim_end_matches('m')
                .parse::<usize>()
                .unwrap_or(1);
            num * 1024 * 1024
        } else {
            normalized.parse::<usize>().unwrap_or(2048)
        }
    }

    pub fn separate_reasoning(
        raw: &str,
        custom_start: Option<&str>,
        custom_end: Option<&str>,
    ) -> (Option<String>, String, usize) {
        if raw.is_empty() {
            return (None, String::new(), 0);
        }

        // 1. If explicit markers are provided, apply them
        if let (Some(start), Some(end)) = (custom_start, custom_end) {
            if !start.is_empty() && !end.is_empty() {
                if let Some(start_pos) = raw.find(start) {
                    let reasoning_start = start_pos + start.len();
                    if let Some(end_offset) = raw[reasoning_start..].find(end) {
                        let end_pos = reasoning_start + end_offset;
                        let reasoning = raw[reasoning_start..end_pos].trim().to_string();
                        let before = &raw[..start_pos];
                        let after = &raw[end_pos + end.len()..];
                        let clean = format!("{}{}", before, after).trim().to_string();
                        let tokens = (reasoning.len() / 4)
                            .max(reasoning.split_whitespace().count())
                            .max(1);
                        return (Some(reasoning), clean, tokens);
                    } else {
                        let reasoning = raw[reasoning_start..].trim().to_string();
                        let clean = raw[..start_pos].trim().to_string();
                        let tokens = (reasoning.len() / 4)
                            .max(reasoning.split_whitespace().count())
                            .max(1);
                        return (Some(reasoning), clean, tokens);
                    }
                } else if let Some(end_pos) = raw.find(end) {
                    let reasoning = raw[..end_pos].trim().to_string();
                    let clean = raw[end_pos + end.len()..].trim().to_string();
                    let tokens = (reasoning.len() / 4)
                        .max(reasoning.split_whitespace().count())
                        .max(1);
                    return (Some(reasoning), clean, tokens);
                }
            }
        }

        // 2. Generic structural delimiter detection (<TAG> ... </TAG>)
        let trimmed_start = raw.trim_start();
        let leading_ws_len = raw.len() - trimmed_start.len();

        if let Some(first_char) = trimmed_start.chars().next() {
            if first_char == '<' {
                if let Some(close_idx) = trimmed_start.find('>') {
                    let open_tag = &trimmed_start[..=close_idx];
                    if !open_tag.starts_with("</") && !open_tag.contains(' ') && open_tag.len() > 2 {
                        let tag_content = &open_tag[1..open_tag.len() - 1];
                        let expected_close = format!("</{}>", tag_content);
                        let reasoning_start = leading_ws_len + open_tag.len();

                        if let Some(end_offset) = raw[reasoning_start..].find(&expected_close) {
                            let end_pos = reasoning_start + end_offset;
                            let reasoning = raw[reasoning_start..end_pos].trim().to_string();
                            let after = &raw[end_pos + expected_close.len()..];
                            let clean = after.trim().to_string();
                            let tokens = (reasoning.len() / 4)
                                .max(reasoning.split_whitespace().count())
                                .max(1);
                            return (Some(reasoning), clean, tokens);
                        }
                    }
                }
            } else if first_char == '[' {
                if let Some(close_idx) = trimmed_start.find(']') {
                    let open_tag = &trimmed_start[..=close_idx];
                    if !open_tag.starts_with("[/") && !open_tag.contains(' ') && open_tag.len() > 2 {
                        let tag_content = &open_tag[1..open_tag.len() - 1];
                        let expected_close = format!("[/{}]", tag_content);
                        let reasoning_start = leading_ws_len + open_tag.len();

                        if let Some(end_offset) = raw[reasoning_start..].find(&expected_close) {
                            let end_pos = reasoning_start + end_offset;
                            let reasoning = raw[reasoning_start..end_pos].trim().to_string();
                            let after = &raw[end_pos + expected_close.len()..];
                            let clean = after.trim().to_string();
                            let tokens = (reasoning.len() / 4)
                                .max(reasoning.split_whitespace().count())
                                .max(1);
                            return (Some(reasoning), clean, tokens);
                        }
                    }
                }
            }
        }

        (None, raw.to_string(), 0)
    }

    /// Safely calculate maximum suffix length matching a candidate prefix using valid UTF-8 char boundaries
    pub fn calc_safe_hold_len(buf: &str, candidates: &[&str]) -> usize {
        let mut max_hold = 0;
        for &cand in candidates {
            if cand.is_empty() {
                continue;
            }
            for (byte_idx, _) in buf.char_indices().rev() {
                let suffix = &buf[byte_idx..];
                if suffix.len() > cand.len() {
                    break;
                }
                if cand.starts_with(suffix) {
                    max_hold = max_hold.max(suffix.len());
                }
            }
        }
        max_hold
    }
}

/// Unified Real-Time Streaming Reasoning Filter.
/// Enforces parity with reasoning blocks for live SSE token streams.
#[derive(Debug, Clone)]
pub struct StreamingReasoningFilter {
    pub start_tag: Option<String>,
    pub end_tag: Option<String>,
    pub in_think_block: bool,
    pub think_done: bool,
    pub buffer: String,
    pub disabled: bool,
}

impl StreamingReasoningFilter {
    pub fn new(
        start_tag: Option<String>,
        end_tag: Option<String>,
        prompt_starts_in_think: bool,
        disabled: bool,
    ) -> Self {
        let has_markers = start_tag.is_some() || end_tag.is_some();
        let think_done = disabled;
        let in_think_block = !disabled && has_markers && prompt_starts_in_think;
        Self {
            start_tag,
            end_tag,
            in_think_block,
            think_done,
            buffer: String::new(),
            disabled,
        }
    }

    /// Reusable reset for consecutive multi-turn reasoning phases.
    pub fn reset_turn(&mut self, prompt_starts_in_think: bool) {
        let has_markers = self.start_tag.is_some() || self.end_tag.is_some();
        self.think_done = self.disabled;
        self.in_think_block = !self.disabled && has_markers && prompt_starts_in_think;
        self.buffer.clear();
    }

    /// Internal helper to handle streaming inside a think block against end_tag.
    /// Returns (content_delta, reasoning_delta).
    fn handle_end_tag_transition(&mut self, end: &str) -> (Option<String>, Option<String>) {
        if let Some(idx) = self.buffer.find(end) {
            self.in_think_block = false;
            self.think_done = true;
            let reasoning_part = self.buffer[..idx].to_string();
            let answer_part = self.buffer[idx + end.len()..].to_string();
            self.buffer.clear();
            let r = if reasoning_part.is_empty() {
                None
            } else {
                Some(reasoning_part)
            };
            let a = if answer_part.is_empty() {
                None
            } else {
                Some(answer_part)
            };
            (a, r)
        } else {
            let hold_len = StructuralDNA::calc_safe_hold_len(&self.buffer, &[end]);
            if self.buffer.len() > hold_len {
                let split_pos = self.buffer.len() - hold_len;
                let emit = self.buffer[..split_pos].to_string();
                self.buffer = self.buffer[split_pos..].to_string();
                (None, Some(emit))
            } else {
                (None, None)
            }
        }
    }

    /// Processes an incoming token chunk and yields (content_delta, reasoning_delta)
    pub fn process_token(&mut self, token: &str) -> (Option<String>, Option<String>) {
        if self.disabled || self.think_done {
            return (Some(token.to_string()), None);
        }

        self.buffer.push_str(token);

        if self.in_think_block {
            if let Some(ref end) = self.end_tag.clone() {
                return self.handle_end_tag_transition(end);
            } else {
                let emit = std::mem::take(&mut self.buffer);
                self.in_think_block = false;
                self.think_done = true;
                return (Some(emit), None);
            }
        }

        if let Some(ref st) = self.start_tag.clone() {
            if let Some(idx) = self.buffer.find(st.as_str()) {
                self.in_think_block = true;
                let before = self.buffer[..idx].to_string();
                let after = self.buffer[idx + st.len()..].to_string();
                self.buffer = after;

                if let Some(ref end) = self.end_tag.clone() {
                    let (a_opt, r_opt) = self.handle_end_tag_transition(end);
                    let combined_a = match (before.is_empty(), a_opt) {
                        (true, opt) => opt,
                        (false, Some(a)) => Some(format!("{}{}", before, a)),
                        (false, None) => Some(before),
                    };
                    return (combined_a, r_opt);
                }

                let b = if before.is_empty() {
                    None
                } else {
                    Some(before)
                };
                return (b, None);
            } else {
                let trimmed = self.buffer.trim_start();
                let is_prefix_of_start = trimmed.is_empty() || st.starts_with(trimmed);
                let max_wait = st.len() + 4;

                if is_prefix_of_start && self.buffer.len() < max_wait {
                    return (None, None);
                }

                let text = std::mem::take(&mut self.buffer);
                self.think_done = true;
                self.in_think_block = false;
                return (Some(text), None);
            }
        }

        if self.end_tag.is_some() {
            let text = std::mem::take(&mut self.buffer);
            self.think_done = true;
            self.in_think_block = false;
            return (Some(text), None);
        }

        let trimmed = self.buffer.trim_start();
        if (trimmed.starts_with('<') && !trimmed.starts_with("</"))
            || (trimmed.starts_with('[') && !trimmed.starts_with("[/"))
        {
            let delimiter_close = if trimmed.starts_with('<') {
                trimmed.find('>')
            } else {
                trimmed.find(']')
            };
            if let Some(close_idx) = delimiter_close {
                let open_tag = &trimmed[..=close_idx];
                if !open_tag.contains(' ') && open_tag.len() > 2 {
                    let tag_content = &open_tag[1..open_tag.len() - 1];
                    let expected_close = if trimmed.starts_with('<') {
                        format!("</{}>", tag_content)
                    } else {
                        format!("[/{}]", tag_content)
                    };
                    self.start_tag = Some(open_tag.to_string());
                    self.end_tag = Some(expected_close);
                    self.in_think_block = true;
                    let tag_pos = self.buffer.find(open_tag).unwrap_or(0);
                    let after = self.buffer[tag_pos + open_tag.len()..].to_string();
                    self.buffer = after;
                    return (None, None);
                } else {
                    let text = std::mem::take(&mut self.buffer);
                    self.think_done = true;
                    return (Some(text), None);
                }
            } else if self.buffer.len() < 32 {
                return (None, None);
            } else {
                let text = std::mem::take(&mut self.buffer);
                self.think_done = true;
                return (Some(text), None);
            }
        } else {
            let text = std::mem::take(&mut self.buffer);
            self.think_done = true;
            return (Some(text), None);
        }
    }

    /// Flushes any remaining buffer at stream completion
    pub fn flush_final(&mut self) -> (Option<String>, Option<String>) {
        if self.buffer.is_empty() {
            return (None, None);
        }
        let text = std::mem::take(&mut self.buffer);
        if self.in_think_block && !self.think_done {
            (None, Some(text))
        } else {
            (Some(text), None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_reasoning_filter_lifecycle() {
        let mut filter = StreamingReasoningFilter::new(
            Some("<think>".into()),
            Some("</think>".into()),
            false,
            false,
        );

        let (c1, r1) = filter.process_token("<think>reasoning step 1");
        assert_eq!(c1, None);
        assert_eq!(r1, Some("reasoning step 1".into()));

        let (c2, r2) = filter.process_token("</think>Hello world");
        assert_eq!(c2, Some("Hello world".into()));
        assert_eq!(r2, None);

        let (c3, r3) = filter.process_token("!");
        assert_eq!(c3, Some("!".into()));
        assert_eq!(r3, None);
    }
}
