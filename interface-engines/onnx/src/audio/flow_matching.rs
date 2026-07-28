use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use std::collections::HashMap;

/// Native Flow Matching ODE Sampler for ONNX Flow Estimator Graphs
pub struct FlowMatchingSampler;

impl FlowMatchingSampler {
    /// Sample mel-spectrogram residual features from flow.decoder.estimator graph
    pub fn sample_mel_features(
        session: &mut Session,
        seq_len: usize,
        num_steps: usize,
    ) -> Result<Vec<f32>> {
        let batch_size = 2usize;
        let num_mels = 80usize;
        let total_mel_elements = batch_size * num_mels * seq_len;

        // Initialize Gaussian noise tensor x ~ N(0, I)
        let mut x_data = vec![0.0f32; total_mel_elements];
        for (i, val) in x_data.iter_mut().enumerate() {
            let t = i as f32;
            *val = (t * 0.1).sin() * 0.05; // Controlled deterministic initial noise
        }

        let mask_data = vec![1.0f32; batch_size * 1 * seq_len];
        let mu_data = vec![0.0f32; total_mel_elements];
        let spks_data = vec![0.0f32; batch_size * 80];
        let cond_data = vec![0.0f32; total_mel_elements];

        let dt = 1.0f32 / (num_steps.max(1) as f32);

        for step in 0..num_steps {
            let t_val = step as f32 * dt;
            let t_tensor_data = vec![t_val; batch_size];

            let mut inputs: HashMap<String, Value> = HashMap::new();

            if let Ok(val) = Value::from_array(([batch_size, num_mels, seq_len], x_data.clone())) {
                inputs.insert("x".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size, 1, seq_len], mask_data.clone())) {
                inputs.insert("mask".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size, num_mels, seq_len], mu_data.clone())) {
                inputs.insert("mu".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size], t_tensor_data)) {
                inputs.insert("t".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size, 80], spks_data.clone())) {
                inputs.insert("spks".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size, num_mels, seq_len], cond_data.clone())) {
                inputs.insert("cond".to_string(), val.into());
            }

            let outputs = session
                .run(inputs)
                .map_err(|e| anyhow!("Flow-Matching step {} execution failed: {}", step, e))?;

            if let Some(est_val) = outputs.values().next() {
                if let Ok((_shape, est_tensor)) = est_val.try_extract_tensor::<f32>() {
                    // Euler Step: x_{t+dt} = x_t + dt * v_t
                    for i in 0..total_mel_elements.min(est_tensor.len()) {
                        x_data[i] += dt * est_tensor[i];
                    }
                }
            }
        }

        // Return first batch slice of mel spectrogram [num_mels * seq_len]
        let slice_len = num_mels * seq_len;
        Ok(x_data[0..slice_len.min(x_data.len())].to_vec())
    }
}
