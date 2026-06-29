use crate::engine::OnnxEngine;
use neural_core::interfaces::router_contract::EngineError;
use cluaiz_shared::{cluaizInference, UnifiedBackend};
use ort::value::Value;
use ort::session::SessionOutputs;
use anyhow::{Result, anyhow};
use std::io::{Write, Read};

impl UnifiedBackend for OnnxEngine {
    fn generate(&mut self, prompt: &str, max_tokens: usize) -> std::result::Result<String, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.generate_stream(prompt, max_tokens, Box::new(move |token| {
            let _ = tx.send(token);
            true
        })).map_err(|e| e.to_string())?;
        
        let mut final_text = String::new();
        for token in rx {
            final_text.push_str(&token);
        }
        Ok(final_text)
    }

    fn prefill(&mut self, prompt: &str) -> Result<()> {
        let tokenizer = self.tokenizer.as_ref().ok_or_else(|| anyhow!("Tokenizer not loaded"))?;
        let encoding = tokenizer.encode(prompt, false).map_err(|e| anyhow!(e.to_string()))?;
        let ids: Vec<u32> = encoding.get_ids().to_vec();
        
        // Feed in chunks or all at once
        let _ = self.forward_raw(&ids, 0)?;
        Ok(())
    }

    fn evaluate_tps(&self) -> f64 {
        0.0 // Placeholder
    }

    fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        self.execute_text_embedding(text).map_err(|e| anyhow!("{:?}", e))
    }
}

impl cluaizInference for OnnxEngine {
    fn forward_raw(&mut self, input_ids: &[u32], _pos: usize) -> Result<Vec<f32>> {
        let session_arc = self.acquire_session().map_err(|e| anyhow!("{:?}", e))?;
        
        let batch_size = 1;
        let seq_len = input_ids.len();
        let ids: Vec<i64> = input_ids.iter().map(|&x| x as i64).collect();
        let mask: Vec<i64> = vec![1; seq_len];

        let ids_val = Value::from_array(([batch_size, seq_len], ids.clone())).map_err(|_| anyhow!("Bad alloc for input_ids"))?;
        let mask_val = Value::from_array(([batch_size, seq_len], mask)).map_err(|_| anyhow!("Bad alloc for attention_mask"))?;

        let mut locked_session = session_arc.lock().map_err(|_| anyhow!("Mutex Poisoned"))?;

        let outputs: SessionOutputs = tokio::task::block_in_place(|| {
            let mut inputs = std::collections::HashMap::<String, Value>::new();
            inputs.insert("input_ids".to_string(), ids_val.into());
            inputs.insert("attention_mask".to_string(), mask_val.into());
            
            // Inject KV cache if we have it
            if let Some(cache) = &self.active_kv_cache {
                let num_layers = cache.len() / 2;
                for layer_idx in 0..num_layers {
                    let key_name = format!("past_key_values.{}.key", layer_idx);
                    let val_name = format!("past_key_values.{}.value", layer_idx);
                    
                    let k_shape = cache[layer_idx * 2].0.clone();
                    let k_data = cache[layer_idx * 2].1.clone();
                    if let Ok(k_val) = Value::from_array((k_shape, k_data)) {
                        inputs.insert(key_name, k_val.into());
                    }
                    
                    let v_shape = cache[layer_idx * 2 + 1].0.clone();
                    let v_data = cache[layer_idx * 2 + 1].1.clone();
                    if let Ok(v_val) = Value::from_array((v_shape, v_data)) {
                        inputs.insert(val_name, v_val.into());
                    }
                }
            }

            locked_session.run(inputs).map_err(|e: ort::Error| anyhow!("ONNX run failed: {}", e))
        })?;

        // Extract logits
        let mut logits = Vec::new();
        if let Some(logits_val) = outputs.get("logits") {
            if let Ok(logits_tuple) = logits_val.try_extract_tensor::<f32>() {
                let slice = logits_tuple.1;
                // Logits shape is typically [batch, seq_len, vocab_size]
                // We want the last token's logits
                if logits_tuple.0.len() == 3 {
                    let vocab_size = logits_tuple.0[2] as usize;
                    let last_token_start = (seq_len - 1) * vocab_size;
                    logits.extend_from_slice(&slice[last_token_start..last_token_start + vocab_size]);
                }
            }
        }

        // Update KV cache
        let mut new_cache = Vec::new();
        for output_name in outputs.keys() {
            if output_name.starts_with("present.") {
                if let Some(val) = outputs.get(output_name) {
                    if let Ok(tuple) = val.try_extract_tensor::<f32>() {
                        let shape = tuple.0.iter().map(|&x| x as usize).collect();
                        new_cache.push((shape, tuple.1.to_vec()));
                    }
                }
            }
        }
        
        if !new_cache.is_empty() {
            self.active_kv_cache = Some(new_cache);
        }

        Ok(logits)
    }

    fn generate_stream(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        mut callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> Result<()> {
        let tokenizer = self.tokenizer.clone().ok_or_else(|| anyhow!("Tokenizer not loaded"))?;
        let encoding = tokenizer.encode(prompt, false).map_err(|e| anyhow!(e.to_string()))?;
        let mut current_ids: Vec<u32> = encoding.get_ids().to_vec();

        // Prefill
        let mut logits = self.forward_raw(&current_ids, 0)?;

        // Autoregressive generation
        for _ in 0..max_tokens {
            // Greedy sampling (argmax)
            let mut max_val = f32::NEG_INFINITY;
            let mut max_idx = 0;
            for (idx, &val) in logits.iter().enumerate() {
                if val > max_val {
                    max_val = val;
                    max_idx = idx as u32;
                }
            }

            let token_id = max_idx;
            if let Ok(token_str) = tokenizer.decode(&[token_id], false) {
                if !callback(token_str) {
                    break;
                }
            }

            // Only pass the new token for the next forward pass
            logits = self.forward_raw(&[token_id], current_ids.len())?;
            current_ids.push(token_id);
        }

        Ok(())
    }

    fn dump_kv_cache(&mut self, path: &str) -> Result<()> {
        let cache = self.active_kv_cache.as_ref()
            .ok_or_else(|| anyhow!("No KV Cache in memory to dump."))?;

        let mut file = std::fs::File::create(path)?;
        
        // Write header: number of tensors
        let num_tensors = cache.len() as u32;
        file.write_all(&num_tensors.to_le_bytes())?;

        for (shape, data) in cache {
            // Write shape rank
            let rank = shape.len() as u32;
            file.write_all(&rank.to_le_bytes())?;
            // Write shape
            for &dim in shape {
                let d = dim as u64;
                file.write_all(&d.to_le_bytes())?;
            }
            // Write data length
            let data_len = data.len() as u64;
            file.write_all(&data_len.to_le_bytes())?;
            // Write raw f32 bytes
            let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4) };
            file.write_all(bytes)?;
        }
        
        Ok(())
    }

    fn load_kv_cache(&mut self, path: &str) -> Result<()> {
        let mut file = std::fs::File::open(path)?;
        let mut new_cache = Vec::new();

        let mut num_tensors_bytes = [0u8; 4];
        file.read_exact(&mut num_tensors_bytes)?;
        let num_tensors = u32::from_le_bytes(num_tensors_bytes);

        for _ in 0..num_tensors {
            let mut rank_bytes = [0u8; 4];
            file.read_exact(&mut rank_bytes)?;
            let rank = u32::from_le_bytes(rank_bytes);

            let mut shape = Vec::with_capacity(rank as usize);
            for _ in 0..rank {
                let mut dim_bytes = [0u8; 8];
                file.read_exact(&mut dim_bytes)?;
                shape.push(u64::from_le_bytes(dim_bytes) as usize);
            }

            let mut len_bytes = [0u8; 8];
            file.read_exact(&mut len_bytes)?;
            let data_len = u64::from_le_bytes(len_bytes) as usize;

            let mut bytes = vec![0u8; data_len * 4]; // f32 is 4 bytes
            file.read_exact(&mut bytes)?;
            
            let data: Vec<f32> = unsafe {
                std::slice::from_raw_parts(bytes.as_ptr() as *const f32, data_len)
            }.to_vec();
            new_cache.push((shape, data));
        }

        self.active_kv_cache = Some(new_cache);
        Ok(())
    }
}
