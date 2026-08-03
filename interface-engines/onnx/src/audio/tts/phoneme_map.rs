use std::collections::HashMap;
use std::path::Path;

/// Phoneme ID Map Parser for Piper/VITS ONNX TTS Models
///
/// Parses `phoneme_id_map` from Piper config files (`.onnx.json` or `config.json`).
/// Maps text characters to model-specific integer IDs for ONNX session input.
///
/// [FACT] Piper config format: `"phoneme_id_map": {"a": [1], "b": [2], " ": [3], ...}`
/// [FACT] Some configs use flat format: `"phoneme_id_map": {"a": 1, "b": 2, ...}`
pub struct PhonemeMap {
    char_to_ids: HashMap<char, Vec<i64>>,
    pad_id: i64,
    bos_id: i64,
    eos_id: i64,
}

impl PhonemeMap {
    /// Load phoneme_id_map or tokenizer vocab from model directory config files.
    /// Scans for `.onnx.json`, `config.json`, `tokenizer.json` in the model directory.
    pub fn from_model_dir(model_dir: &Path) -> Option<Self> {
        if !model_dir.exists() || !model_dir.is_dir() {
            return None;
        }

        // Pass 1: Scan all JSON files in model dir for phoneme_id_map
        let entries = std::fs::read_dir(model_dir).ok()?;
        let mut json_paths = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if ext == "json" || name.ends_with(".onnx.json") {
                if let Some(map) = Self::try_parse_file(&path) {
                    return Some(map);
                }
                json_paths.push(path);
            }
        }

        // Pass 2: Fallback to parsing tokenizer.json model.vocab
        let tok_path = model_dir.join("tokenizer.json");
        if tok_path.exists() {
            if let Some(map) = Self::try_parse_tokenizer_file(&tok_path) {
                return Some(map);
            }
        }

        None
    }

    fn try_parse_tokenizer_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        let vocab_obj = json
            .get("model")
            .and_then(|m| m.get("vocab"))
            .or_else(|| json.get("vocab"))?
            .as_object()?;

        let mut char_to_ids: HashMap<char, Vec<i64>> = HashMap::new();
        let pad_id: i64 = 0;
        let mut bos_id: i64 = 0;
        let mut eos_id: i64 = 0;

        for (key, value) in vocab_obj {
            if let Some(id) = value.as_i64() {
                if key == "$" {
                    bos_id = id;
                    eos_id = id;
                }
                for ch in key.chars() {
                    char_to_ids.insert(ch, vec![id]);
                }
            }
        }

        if char_to_ids.is_empty() {
            return None;
        }

        eprintln!(
            "📖 [PhonemeMap] Loaded {} tokenizer vocab entries from {:?}",
            char_to_ids.len(),
            path.file_name().unwrap_or_default()
        );

        Some(Self {
            char_to_ids,
            pad_id,
            bos_id,
            eos_id,
        })
    }

    fn try_parse_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        let phoneme_map = json.get("phoneme_id_map")?;
        let obj = phoneme_map.as_object()?;

        let mut char_to_ids: HashMap<char, Vec<i64>> = HashMap::new();
        let mut pad_id: i64 = 0;
        let mut bos_id: i64 = 1;
        let mut eos_id: i64 = 2;

        for (key, value) in obj {
            let ids: Vec<i64> = match value {
                // Array format: {"a": [1]} or {"a": [1, 2]}
                serde_json::Value::Array(arr) => {
                    arr.iter().filter_map(|v| v.as_i64()).collect()
                }
                // Flat format: {"a": 1}
                serde_json::Value::Number(n) => {
                    if let Some(id) = n.as_i64() {
                        vec![id]
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            if ids.is_empty() {
                continue;
            }

            // Handle special control characters
            if key == "^" || key == "<bos>" || key == "<s>" {
                bos_id = ids[0];
            } else if key == "$" || key == "<eos>" || key == "</s>" {
                eos_id = ids[0];
            } else if key == "_" || key == "<pad>" || key == "<blank>" {
                pad_id = ids[0];
            }

            // Map each character in the key string
            for ch in key.chars() {
                char_to_ids.insert(ch, ids.clone());
            }
        }

        if char_to_ids.is_empty() {
            return None;
        }

        eprintln!(
            "📖 [PhonemeMap] Loaded {} character mappings from {:?} (pad={}, bos={}, eos={})",
            char_to_ids.len(),
            path.file_name().unwrap_or_default(),
            pad_id, bos_id, eos_id
        );

        Some(Self {
            char_to_ids,
            pad_id,
            bos_id,
            eos_id,
        })
    }

    /// Convert text string to phoneme ID sequence.
    /// Wraps with BOS/EOS tokens and inserts pad between characters.
    ///
    /// For Piper models trained on graphemes (character-level), this produces
    /// correct input. For phoneme-trained models, espeak-ng G2P would be
    /// needed upstream (not implemented yet).
    pub fn text_to_ids(&self, text: &str) -> Vec<i64> {
        let mut ids = Vec::with_capacity(text.len() * 2 + 2);
        ids.push(self.bos_id);

        for ch in text.chars() {
            if let Some(char_ids) = self.char_to_ids.get(&ch) {
                ids.extend_from_slice(char_ids);
                ids.push(self.pad_id);
            } else {
                // Unknown character: try lowercase variant
                let lower = ch.to_lowercase().next().unwrap_or(ch);
                if let Some(char_ids) = self.char_to_ids.get(&lower) {
                    ids.extend_from_slice(char_ids);
                    ids.push(self.pad_id);
                }
                // Skip completely unknown characters silently
            }
        }

        ids.push(self.eos_id);
        ids
    }

    /// Returns the pad token ID
    pub fn pad_id(&self) -> i64 {
        self.pad_id
    }
}
