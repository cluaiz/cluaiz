use ort::session::Session;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Dynamic manifest and metadata loader for all ONNX TTS model families.
/// Enforces zero-hardcoding law: all sample rates, scales, token maps,
/// and dimensions are parsed directly from installed model assets and ONNX graph headers.
#[derive(Debug, Clone, Default)]
pub struct TtsModelManifest {
    pub sample_rate: Option<u32>,
    pub num_channels: Option<u32>,
    pub phoneme_id_map: Option<HashMap<String, i64>>,
    pub noise_scale: Option<f32>,
    pub length_scale: Option<f32>,
    pub noise_scale_w: Option<f32>,
    pub semantic_begin_id: Option<i64>,
    pub num_codebooks: Option<usize>,
    pub codebook_size: Option<usize>,
    pub mel_channels: Option<usize>,
    pub tokens_map: Option<HashMap<String, i64>>,
    pub add_blank: Option<bool>,
    pub num_speakers: Option<usize>,
    pub model_type: Option<String>,
    pub voice_names: Vec<String>,
}

impl TtsModelManifest {
    /// Inspect and parse ONNX Graph Metadata headers directly from the active session
    pub fn parse_from_session(session: &Session, model_dir: &Path) -> Self {
        let mut manifest = Self::parse_from_dir(model_dir);

        if let Ok(meta) = session.metadata() {
            if let Some(sr_str) = meta.custom("sample_rate") {
                if let Ok(sr) = sr_str.parse::<u32>() {
                    eprintln!("📖 [Metadata Parser] Found sample_rate={} in ONNX header metadata", sr);
                    manifest.sample_rate = Some(sr);
                }
            }
            if let Some(blank_str) = meta.custom("add_blank") {
                manifest.add_blank = Some(blank_str == "1" || blank_str.to_lowercase() == "true");
            }
            if let Some(mtype) = meta.custom("model_type").or_else(|| meta.custom("comment")) {
                manifest.model_type = Some(mtype);
            }
            if let Some(spk_str) = meta.custom("num_speakers") {
                if let Ok(spk) = spk_str.parse::<usize>() {
                    manifest.num_speakers = Some(spk);
                }
            }
        }

        manifest
    }

    /// Inspect and parse all manifest files (.json, .yaml, .txt) present in a model directory.
    pub fn parse_from_dir(model_dir: &Path) -> Self {
        let mut manifest = Self::default();

        if !model_dir.exists() {
            return manifest;
        }

        // Recursively inspect JSON / YAML files in model_dir
        Self::scan_directory_manifests(model_dir, &mut manifest);

        // Scan voices/ and voice_styles/ for voice asset inventory
        let voices_dir = model_dir.join("voices");
        if voices_dir.exists() && voices_dir.is_dir() {
            if let Ok(rd) = fs::read_dir(&voices_dir) {
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".bin") || name.ends_with(".json") {
                        let voice_stem = name.trim_end_matches(".bin").trim_end_matches(".json").to_string();
                        manifest.voice_names.push(voice_stem);
                    }
                }
            }
        }

        let styles_dir = model_dir.join("voice_styles");
        if styles_dir.exists() && styles_dir.is_dir() {
            if let Ok(rd) = fs::read_dir(&styles_dir) {
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".json") {
                        let voice_stem = name.trim_end_matches(".json").to_string();
                        manifest.voice_names.push(voice_stem);
                    }
                }
            }
        }

        // Inspect tokens.txt if present (Matcha / Sherpa-ONNX)
        let tokens_txt_path = model_dir.join("tokens.txt");
        if tokens_txt_path.exists() {
            if let Ok(content) = fs::read_to_string(&tokens_txt_path) {
                let mut map = HashMap::new();
                for line in content.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(id) = parts[1].parse::<i64>() {
                            map.insert(parts[0].to_string(), id);
                        }
                    }
                }
                if !map.is_empty() {
                    manifest.tokens_map = Some(map);
                }
            }
        }

        manifest
    }

    fn scan_directory_manifests(dir: &Path, manifest: &mut TtsModelManifest) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Recursively scan 1 level down
                    if path.file_name().map(|n| n == "voices" || n == "voice_styles").unwrap_or(false) {
                        continue;
                    }
                    Self::scan_directory_manifests(&path, manifest);
                    continue;
                }
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if fname.ends_with(".json") || fname.ends_with(".yaml") || fname.ends_with(".yml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                            // Extract sample_rate
                            if manifest.sample_rate.is_none() {
                                if let Some(sr) = val.get("sample_rate").and_then(|v| v.as_u64()) {
                                    manifest.sample_rate = Some(sr as u32);
                                } else if let Some(sr) = val.get("codec_sample_rate").and_then(|v| v.as_u64()) {
                                    manifest.sample_rate = Some(sr as u32);
                                } else if let Some(audio) = val.get("audio") {
                                    if let Some(sr) = audio.get("sample_rate").and_then(|v| v.as_u64()) {
                                        manifest.sample_rate = Some(sr as u32);
                                    }
                                } else if let Some(ae) = val.get("ae") {
                                    if let Some(sr) = ae.get("sample_rate").and_then(|v| v.as_u64()) {
                                        manifest.sample_rate = Some(sr as u32);
                                    }
                                }
                            }

                            // Extract mel_channels
                            if manifest.mel_channels.is_none() {
                                if let Some(mc) = val.get("mel_channels").or_else(|| val.get("num_mels")).and_then(|v| v.as_u64()) {
                                    manifest.mel_channels = Some(mc as usize);
                                }
                            }

                            // Extract inference scale parameters (Piper/VITS)
                            if let Some(inf) = val.get("inference") {
                                if let Some(ns) = inf.get("noise_scale").and_then(|v| v.as_f64()) {
                                    manifest.noise_scale = Some(ns as f32);
                                }
                                if let Some(ls) = inf.get("length_scale").and_then(|v| v.as_f64()) {
                                    manifest.length_scale = Some(ls as f32);
                                }
                                if let Some(nw) = inf.get("noise_w").and_then(|v| v.as_f64()) {
                                    manifest.noise_scale_w = Some(nw as f32);
                                }
                            }

                            // Extract phoneme_id_map
                            if manifest.phoneme_id_map.is_none() {
                                if let Some(pmap) = val.get("phoneme_id_map").and_then(|v| v.as_object()) {
                                    let mut map = HashMap::new();
                                    for (k, v) in pmap {
                                        if let Some(id) = v.as_i64() {
                                            map.insert(k.clone(), id);
                                        } else if let Some(arr) = v.as_array() {
                                            if let Some(id) = arr.first().and_then(|i| i.as_i64()) {
                                                map.insert(k.clone(), id);
                                            }
                                        }
                                    }
                                    if !map.is_empty() {
                                        manifest.phoneme_id_map = Some(map);
                                    }
                                }
                            }

                            // Extract tokenizer.json vocab if phoneme_id_map not present
                            if manifest.phoneme_id_map.is_none() && fname == "tokenizer.json" {
                                if let Some(vocab_obj) = val.get("model").and_then(|m| m.get("vocab")).or_else(|| val.get("vocab")).and_then(|v| v.as_object()) {
                                    let mut map = HashMap::new();
                                    for (k, v) in vocab_obj {
                                        if let Some(id) = v.as_i64() {
                                            map.insert(k.clone(), id);
                                        }
                                    }
                                    if !map.is_empty() {
                                        manifest.phoneme_id_map = Some(map);
                                    }
                                }
                            }

                            // Extract Audio8 / Codec-LM fields
                            if let Some(s_begin) = val.get("semantic_begin_id").and_then(|v| v.as_i64()) {
                                manifest.semantic_begin_id = Some(s_begin);
                            }
                            if let Some(n_cb) = val.get("num_codebooks").and_then(|v| v.as_u64()) {
                                manifest.num_codebooks = Some(n_cb as usize);
                            }
                            if let Some(cb_sz) = val.get("codebook_size").and_then(|v| v.as_u64()) {
                                manifest.codebook_size = Some(cb_sz as usize);
                            }
                        }
                    }
                }
            }
        }
    }
}
