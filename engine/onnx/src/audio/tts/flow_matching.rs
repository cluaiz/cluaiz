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
        engine: &crate::engine::OnnxEngine,
        tokenizer: Option<&tokenizers::Tokenizer>,
        seq_len: usize,
        num_steps: usize,
    ) -> Result<Vec<f32>> {
        Self::sample_mel_features_with_text(session, engine, tokenizer, "", seq_len, num_steps)
    }

    /// Sample mel-spectrogram residual features with text conditioning
    pub fn sample_mel_features_with_text(
        session: &mut Session,
        engine: &crate::engine::OnnxEngine,
        tokenizer: Option<&tokenizers::Tokenizer>,
        text_input: &str,
        seq_len: usize,
        num_steps: usize,
    ) -> Result<Vec<f32>> {
        let num_mels = 80usize;

        match Self::run_ode_sampling(session, engine, tokenizer, text_input, 1, num_mels, seq_len, num_steps) {
            Ok(res) => Ok(res),
            Err(e1) => {
                let err_str = e1.to_string();
                if err_str.contains("batch") || err_str.contains("shape") || err_str.contains("dim") || err_str.contains("Invalid rank") || err_str.contains("failed") {
                    Self::run_ode_sampling(session, engine, tokenizer, text_input, 2, num_mels, seq_len, num_steps)
                } else {
                    Err(e1)
                }
            }
        }
    }

    fn run_ode_sampling(
        session: &mut Session,
        engine: &crate::engine::OnnxEngine,
        tokenizer: Option<&tokenizers::Tokenizer>,
        text_input: &str,
        batch_size: usize,
        num_mels: usize,
        seq_len: usize,
        num_steps: usize,
    ) -> Result<Vec<f32>> {
        let total_mel_elements = batch_size * num_mels * seq_len;

        let mut x_data = vec![0.0f32; total_mel_elements];
        for i in (0..total_mel_elements).step_by(2) {
            let u1 = ((i + 1) as f32 * 0.1234567).fract().max(1e-6);
            let u2 = ((i + 2) as f32 * 0.7654321).fract();
            let radius = (-2.0 * u1.ln()).sqrt() * 0.15;
            let theta = 2.0 * std::f32::consts::PI * u2;
            x_data[i] = radius * theta.cos();
            if i + 1 < total_mel_elements {
                x_data[i + 1] = radius * theta.sin();
            }
        }

        let mask_data = vec![1.0f32; batch_size * 1 * seq_len];

        let mut mu_data = vec![0.0f32; total_mel_elements];
        let mut cond_data = vec![0.0f32; total_mel_elements];
        let mut text_encoder_used = false;

        if let Some(text_enc_arc) = &engine.text_encoder_session {
            if let Ok(mut text_sess) = text_enc_arc.lock() {
                let token_ids: Vec<i64> = if let Some(tok) = tokenizer {
                    if let Ok(encoding) = tok.encode(text_input, false) {
                        encoding.get_ids().iter().map(|&id| id as i64).collect()
                    } else {
                        text_input.bytes().map(|b| b as i64).collect()
                    }
                } else {
                    text_input.bytes().map(|b| b as i64).collect()
                };

                if !token_ids.is_empty() {
                    let enc_seq_len = token_ids.len().max(1);
                    let mut text_inputs: HashMap<String, Value> = HashMap::new();
                    for input in text_sess.inputs() {
                        let name = input.name().to_string();
                        let input_debug = format!("{:?}", input);
                        let is_int = input_debug.contains("Int64") || input_debug.contains("int64") || input_debug.contains("Int32") || input_debug.contains("int32");

                        if name.contains("len") {
                            if is_int {
                                if let Ok(val) = Value::from_array(([1usize], vec![enc_seq_len as i64])) {
                                    text_inputs.insert(name.clone(), val.into());
                                }
                            } else if let Ok(val) = Value::from_array(([1usize], vec![enc_seq_len as f32])) {
                                text_inputs.insert(name.clone(), val.into());
                            }
                        } else if is_int {
                            if let Ok(val) = Value::from_array(([1usize, enc_seq_len], token_ids.clone())) {
                                text_inputs.insert(name.clone(), val.into());
                            }
                        } else {
                            let token_ids_f32: Vec<f32> = token_ids.iter().map(|&x| x as f32).collect();
                            if let Ok(val) = Value::from_array(([1usize, enc_seq_len], token_ids_f32)) {
                                text_inputs.insert(name.clone(), val.into());
                            }
                        }
                    }

                    match text_sess.run(text_inputs) {
                        Ok(outputs) => {
                            if let Some(out_val) = outputs.values().next() {
                                if let Ok((shape, tensor)) = out_val.try_extract_tensor::<f32>() {
                                    let emb_dim = if shape.len() == 3 { shape[2] as usize } else { 80 };
                                    let out_seq_len = if shape.len() >= 2 { shape[1] as usize } else { enc_seq_len };
                                    for b in 0..batch_size {
                                        for mel in 0..num_mels {
                                            for t in 0..seq_len {
                                                let idx = b * (num_mels * seq_len) + mel * seq_len + t;
                                                let src_t = (t * out_seq_len) / seq_len.max(1);
                                                let src_mel = (mel * emb_dim) / num_mels.max(1);
                                                let src_idx = src_t * emb_dim + src_mel;
                                                let val = if src_idx < tensor.len() { tensor[src_idx] } else { 0.0 };
                                                mu_data[idx] = val;
                                                cond_data[idx] = val;
                                            }
                                        }
                                    }
                                    text_encoder_used = true;
                                }
                            }
                        }
                        Err(e) => eprintln!("❌ [Text Encoder] Graph run error: {}", e),
                    }
                }
            }
        }

        if !text_encoder_used {
            return Err(anyhow!(
                "PackageContractException: Flow/Matcha/CosyVoice estimator requires a real text/acoustic encoder output for mu/cond. Refusing to synthesize token-derived fake mel audio."
            ));
        }

        let spks_data = vec![0.01f32; batch_size * 80];
        let num_steps_safe = num_steps.max(1);
        let dt = 1.0f32 / (num_steps_safe as f32);

        for step in 0..num_steps_safe {
            let t_val = step as f32 * dt;
            let t_tensor_data = vec![t_val; batch_size];

            let mut inputs: HashMap<String, Value> = HashMap::new();

            if let Ok(val) = Value::from_array(([batch_size, num_mels, seq_len], x_data.clone())) {
                inputs.insert("x".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size, 1usize, seq_len], mask_data.clone())) {
                inputs.insert("mask".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size, num_mels, seq_len], mu_data.clone())) {
                inputs.insert("mu".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size], t_tensor_data)) {
                inputs.insert("t".to_string(), val.into());
            }
            if let Ok(val) = Value::from_array(([batch_size, 80usize], spks_data.clone())) {
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
                    let min_len = total_mel_elements.min(est_tensor.len());
                    for i in 0..min_len {
                        x_data[i] += dt * est_tensor[i];
                    }
                }
            }
        }

        let slice_len = num_mels * seq_len;
        Ok(x_data[0..slice_len.min(x_data.len())].to_vec())
    }
}
