use crate::engine::OnnxEngine;
use neural_core::interfaces::router_contract::EngineError;
use image::imageops::FilterType;
use tracing::info;

/// 👁️ Vision Preprocessor
/// Extracts and normalizes raw image pixels into the mathematical tensor format 
/// required by CLIP / Vision Gatekeepers (1x3x224x224).
pub fn preprocess_image_for_clip(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    info!("🖼️ [Vision-Router] Decoding raw image bytes for visual ingestion...");
    let img = image::load_from_memory(bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode image bytes: {}", e))?;
        
    // 1. Resize to 224x224 (Standard CLIP dimension) without relying on external Python bloat.
    let img_resized = img.resize_exact(224, 224, FilterType::Triangle); 
    let rgb_img = img_resized.to_rgb8();
    
    // 2. Normalize and format as [1, 3, 224, 224] tensor
    let mean = [0.48145466, 0.4578275, 0.40821073];
    let std = [0.26862954, 0.26130258, 0.27577711];
    
    let mut tensor = vec![0.0f32; 3 * 224 * 224];
    
    info!("🔢 [Vision-Router] Normalizing pixels into mathematical embedding space (CHW format)...");
    for (x, y, pixel) in rgb_img.enumerate_pixels() {
        let r = pixel[0] as f32 / 255.0;
        let g = pixel[1] as f32 / 255.0;
        let b = pixel[2] as f32 / 255.0;
        
        let r = (r - mean[0]) / std[0];
        let g = (g - mean[1]) / std[1];
        let b = (b - mean[2]) / std[2];
        
        let c_stride = 224 * 224;
        let idx = (y as usize * 224) + x as usize;
        
        tensor[idx] = r;
        tensor[idx + c_stride] = g;
        tensor[idx + 2 * c_stride] = b;
    }
    
    info!("✅ [Vision-Router] Mathematical Vision Tensor generated successfully.");
    Ok(tensor)
}

impl OnnxEngine {
    pub fn execute_vision_embedding(&self, bytes: &[u8]) -> Result<Vec<f32>, EngineError> {
        // 🔢 Track active inferences for hot swap safety
        self.active_inferences.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let result = self._execute_vision_inner(bytes);
        self.active_inferences.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        result
    }

    fn _execute_vision_inner(&self, bytes: &[u8]) -> Result<Vec<f32>, EngineError> {
        let tensor_data = preprocess_image_for_clip(bytes)
            .map_err(|e| EngineError::EmbeddingFailed(e.to_string()))?;
            
        let input_value = ort::value::Value::from_array(([1usize, 3usize, 224usize, 224usize], tensor_data))
            .map_err(|e| EngineError::EmbeddingFailed(format!("Failed to create value: {}", e)))?;

        // 🏊 Acquire a free session from the pool
        let session_arc = self.acquire_session()?;

        info!("🚀 [Vision-Router] Injecting tensor into ONNX Engine...");
        let mut session = session_arc.lock().map_err(|_| EngineError::Internal("Mutex Poisoned".into()))?;
        let required_inputs: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
        let output_names: Vec<String> = session.outputs().iter().map(|o| o.name().to_string()).collect();
        let target_idx = output_names.iter().position(|name| name == "image_embeds" || name == "embedding").unwrap_or(0);
        
        let outputs = tokio::task::block_in_place(|| {
            if required_inputs.contains(&"input_ids".to_string()) && required_inputs.contains(&"attention_mask".to_string()) {
                info!("📡 [ONNX-Vision] Graph expects text inputs (input_ids/attention_mask). Injecting dummy sequence.");
                let dummy_ids = vec![0i64; 77];
                let dummy_mask = vec![0i64; 77];
                let ids_val = ort::value::Value::from_array(([1usize, 77usize], dummy_ids)).unwrap();
                let mask_val = ort::value::Value::from_array(([1usize, 77usize], dummy_mask)).unwrap();
                
                let inputs = ort::inputs!["pixel_values" => input_value, "input_ids" => ids_val, "attention_mask" => mask_val];
                session.run(inputs).map_err(|e| EngineError::EmbeddingFailed(format!("ONNX execution failed: {}", e)))
            } else {
                let inputs = ort::inputs!["pixel_values" => input_value];
                session.run(inputs).map_err(|e| EngineError::EmbeddingFailed(format!("ONNX execution failed: {}", e)))
            }
        })?;

        let output_tensor = outputs[target_idx].try_extract_tensor::<f32>()
            .map_err(|e| EngineError::EmbeddingFailed(format!("Failed to extract output: {}", e)))?;

        let embedding = output_tensor.1.to_vec();
        
        info!("✅ [Vision-Router] Real ONNX Math Vector Extracted. Dimensions: {}", embedding.len());
        Ok(embedding)
    }
}
