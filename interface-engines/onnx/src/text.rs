use crate::engine::OnnxEngine;
use neural_core::interfaces::router_contract::EngineError;
use ort::value::Value;
use ort::session::SessionOutputs;

impl OnnxEngine {
    pub fn execute_text_embedding(&self, text: &str) -> Result<Vec<f32>, EngineError> {
        // 🔢 Track active inferences for hot swap safety
        self.active_inferences.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let result = self._execute_text_embedding_inner(text);

        self.active_inferences.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        result
    }

    fn _execute_text_embedding_inner(&self, text: &str) -> Result<Vec<f32>, EngineError> {
        // 🏊 Acquire a free session from the pool
        let session_arc = self.acquire_session()?;
        let tokenizer = self.tokenizer.as_ref().ok_or_else(|| EngineError::Internal("Tokenizer not loaded".to_string()))?;

        // 1. Tokenize Text
        let encoding = tokenizer.encode(text, true).map_err(|e| EngineError::EmbeddingFailed(e.to_string()))?;
        let ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&x| x as i64).collect();
        
        let batch_size = 1;
        let seq_len = ids.len();

        // 2. Create ORT Values using tuple `([shape], Vec<T>)` which implements `OwnedTensorArrayData`
        let ids_val = Value::from_array(([batch_size, seq_len], ids)).map_err(|_| EngineError::Internal("Bad Alloc".into()))?;
        let mask_val = Value::from_array(([batch_size, seq_len], mask)).map_err(|_| EngineError::Internal("Bad Alloc".into()))?;

        // 3. Run Inference inside block_in_place to avoid blocking the async executor
        let mut locked_session = session_arc.lock().map_err(|_| EngineError::Internal("Mutex Poisoned".into()))?;
        
        // Dynamically probe required inputs
        let required_inputs: Vec<String> = locked_session.inputs().iter().map(|i| i.name().to_string()).collect();
        let output_names: Vec<String> = locked_session.outputs().iter().map(|o| o.name().to_string()).collect();
        
        let outputs: SessionOutputs = tokio::task::block_in_place(|| {
            if required_inputs.contains(&"pixel_values".to_string()) && required_inputs.contains(&"token_type_ids".to_string()) {
                tracing::info!("📡 [ONNX-Text] Graph expects pixel_values AND token_type_ids. Injecting dummies.");
                let dummy = vec![0.0f32; 3 * 224 * 224];
                let dummy_val = Value::from_array(([1usize, 3usize, 224usize, 224usize], dummy)).unwrap();
                let dummy_types = vec![0i64; seq_len];
                let token_types_val = Value::from_array(([batch_size, seq_len], dummy_types)).unwrap();
                let inputs = ort::inputs!["input_ids" => ids_val, "attention_mask" => mask_val, "token_type_ids" => token_types_val, "pixel_values" => dummy_val];
                locked_session.run(inputs).map_err(|e: ort::Error| EngineError::EmbeddingFailed(e.to_string()))
            } else if required_inputs.contains(&"pixel_values".to_string()) {
                tracing::info!("📡 [ONNX-Text] Graph expects pixel_values. Injecting dummy image tensor.");
                let dummy = vec![0.0f32; 3 * 224 * 224];
                let dummy_val = Value::from_array(([1usize, 3usize, 224usize, 224usize], dummy)).unwrap();
                let inputs = ort::inputs!["input_ids" => ids_val, "attention_mask" => mask_val, "pixel_values" => dummy_val];
                locked_session.run(inputs).map_err(|e: ort::Error| EngineError::EmbeddingFailed(e.to_string()))
            } else if required_inputs.contains(&"token_type_ids".to_string()) {
                tracing::info!("📡 [ONNX-Text] Graph expects token_type_ids. Injecting dummy zeros.");
                let dummy_types = vec![0i64; seq_len];
                let token_types_val = Value::from_array(([batch_size, seq_len], dummy_types)).unwrap();
                let inputs = ort::inputs!["input_ids" => ids_val, "attention_mask" => mask_val, "token_type_ids" => token_types_val];
                locked_session.run(inputs).map_err(|e: ort::Error| EngineError::EmbeddingFailed(e.to_string()))
            } else {
                let inputs = ort::inputs!["input_ids" => ids_val, "attention_mask" => mask_val];
                locked_session.run(inputs).map_err(|e: ort::Error| EngineError::EmbeddingFailed(e.to_string()))
            }
        })?;

        // 4. Extract raw tensor and apply Mean Pooling
        let target_idx = output_names.iter().position(|name| name == "text_embeds" || name == "sentence_embedding").unwrap_or(0);
        let embeddings_tuple = outputs[target_idx].try_extract_tensor::<f32>().map_err(|_| EngineError::Internal("Tensor Extract Failed".into()))?;
        
        let shape = &embeddings_tuple.0;
        let slice = embeddings_tuple.1;
        
        let vec = if shape.len() == 2 {
            // Already pooled: [batch_size, hidden_dim]
            slice.to_vec()
        } else if shape.len() == 3 {
            // Unpooled: [batch_size, seq_len, hidden_dim]
            let hidden_dim = shape[2] as usize;
            let actual_seq_len = shape[1] as usize;
            let mut pooled = vec![0.0; hidden_dim];
            for token_idx in 0..actual_seq_len {
                for dim in 0..hidden_dim {
                    pooled[dim] += slice[token_idx * hidden_dim + dim];
                }
            }
            for dim in 0..hidden_dim {
                pooled[dim] /= actual_seq_len as f32;
            }
            pooled
        } else {
            slice.to_vec() // Fallback
        };

        // L2 Normalization (Cosine Similarity Ready)
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        let normalized = vec.into_iter().map(|v| v / norm).collect();

        tracing::info!("⚡ [ONNX-Text] Vector generated successfully! (Dim: {})", seq_len);
        Ok(normalized)
    }
}
