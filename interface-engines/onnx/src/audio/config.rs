use std::path::PathBuf;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub n_mels: usize,
    pub sample_rate: u32,
    pub hop_length: usize,
    pub n_fft: usize,
    pub max_frames: usize,
    pub max_samples: usize,
    pub max_target_positions: Option<usize>,
    pub start_of_transcript: i64,
    pub transcribe_token: i64,
    pub translate_token: i64,
    pub no_timestamps_token: i64,
    pub end_of_text_token: i64,
}

impl AudioConfig {
    pub fn from_model_dir(model_dir: &Option<PathBuf>) -> Self {
        let mut n_mels = 80;
        let mut sample_rate = 16000;
        let mut max_frames = 3000;

        let mut start_of_transcript = 50258;
        let mut transcribe_token = 50360;
        let mut translate_token = 50359;
        let mut no_timestamps_token = 50364;
        let mut end_of_text_token = 50257;

        let mut max_target_positions: Option<usize> = None;

        if let Some(ref dir) = model_dir {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                                if let Some(target_pos) = json.get("max_target_positions")
                                    .or_else(|| json.get("max_decoder_positions"))
                                    .or_else(|| json.get("max_target_len"))
                                    .and_then(|v| v.as_u64())
                                {
                                    max_target_positions = Some(target_pos as usize);
                                }

                                if let Some(mels) = json.get("num_mel_bins")
                                    .or_else(|| json.get("n_mels"))
                                    .or_else(|| json.get("feature_size"))
                                    .and_then(|v| v.as_u64())
                                {
                                    n_mels = mels as usize;
                                }
                                if let Some(sr) = json.get("sampling_rate")
                                    .or_else(|| json.get("sample_rate"))
                                    .and_then(|v| v.as_u64())
                                {
                                    sample_rate = sr as u32;
                                }
                                if let Some(frames) = json.get("nb_max_frames")
                                    .or_else(|| json.get("max_source_positions"))
                                    .and_then(|v| v.as_u64())
                                {
                                    max_frames = frames as usize;
                                } else if let Some(n_samples) = json.get("n_samples").and_then(|v| v.as_u64()) {
                                    max_frames = (n_samples / 160) as usize;
                                }

                                if let Some(s) = json.get("decoder_start_token_id").and_then(|v| v.as_i64()) {
                                    start_of_transcript = s;
                                }
                                if let Some(eos) = json.get("eos_token_id").and_then(|v| v.as_i64()) {
                                    end_of_text_token = eos;
                                }
                            }
                        }
                    }
                }
            }

            // Second pass: Check tokenizer-specific config files for Whisper v3/Turbo compatibility
            for filename in &["special_tokens_map.json", "tokenizer_config.json"] {
                let path = dir.join(filename);
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(json) = serde_json::from_str::<Value>(&content) {
                        if let Some(eos_id) = json.get("eos_token_id").and_then(|v| v.as_i64()) {
                            end_of_text_token = eos_id;
                        }
                        if let Some(bos_id) = json.get("bos_token_id")
                            .or_else(|| json.get("decoder_start_token_id"))
                            .and_then(|v| v.as_i64())
                        {
                            start_of_transcript = bos_id;
                        }
                    }
                }
            }
        }

        let hop_length = 160;
        let n_fft = 400;
        let max_samples = sample_rate as usize * 30;

        Self {
            n_mels,
            sample_rate,
            hop_length,
            n_fft,
            max_frames,
            max_samples,
            max_target_positions,
            start_of_transcript,
            transcribe_token,
            translate_token,
            no_timestamps_token,
            end_of_text_token,
        }
    }
}
