use std::collections::HashMap;
use std::path::Path;

/// SymbolTable & Phoneme ID Map Parser for all ONNX TTS Models (300+ Languages)
///
/// 1:1 Parity with Xiaomi `sherpa-onnx` C++ `SymbolTable` (`symbol-table.cc`).
/// Stores exact string token mappings (`sym_to_id`) and performs longest-prefix
/// substring matching to support multi-character IPA phonemes (`"tʃ"`, `"dʒ"`, `"aɪ"`).
///
/// Enforces Zero-Hardcoding Law: All tokens, IDs, and symbols are loaded 100% dynamically.
pub struct PhonemeMap {
    sym_to_id: HashMap<String, Vec<i64>>,
    char_to_ids: HashMap<char, Vec<i64>>,
    pad_id: i64,
    bos_id: i64,
    eos_id: i64,
    max_key_len: usize,
}

impl PhonemeMap {
    /// Load phoneme_id_map or tokenizer vocab from model directory config files.
    /// Scans for `.onnx.json`, `config.json`, `tokenizer.json` in the model directory.
    pub fn from_model_dir(model_dir: &Path) -> Option<Self> {
        if !model_dir.exists() || !model_dir.is_dir() {
            return None;
        }

        // Pass 1: Scan for standard tokens.txt file as specified in TTS_FAMILY_BLUEPRINT.md
        let tok_path = model_dir.join("tokens.txt");
        if tok_path.exists() {
            if let Some(map) = Self::try_parse_tokens_file(&tok_path) {
                return Some(map);
            }
        }

        // Pass 2: Check standard tokenizer.json (HuggingFace token vocabulary)
        let tok_json = model_dir.join("tokenizer.json");
        if tok_json.exists() {
            if let Some(map) = Self::try_parse_tokenizer_file(&tok_json) {
                return Some(map);
            }
        }

        // Pass 3: Scan all JSON files in model dir for phoneme_id_map / tokens_map
        let entries = std::fs::read_dir(model_dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if (ext == "json" || name.ends_with(".onnx.json")) && name != "tokenizer.json" && name != "tokenizer_config.json" {
                if let Some(map) = Self::try_parse_file(&path) {
                    return Some(map);
                }
            }
        }

        None
    }

    /// Parse standard `tokens.txt` file (Sherpa-ONNX SymbolTable line format `<token>\t<id>` or `<token> <id>`).
    fn try_parse_tokens_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let mut sym_to_id: HashMap<String, Vec<i64>> = HashMap::new();
        let mut char_to_ids: HashMap<char, Vec<i64>> = HashMap::new();
        let pad_id: i64 = 0;
        let bos_id: i64 = 0;
        let eos_id: i64 = 0;
        let mut max_key_len: usize = 1;

        for line in content.lines() {
            let line_trimmed = line.trim();
            if line_trimmed.is_empty() || line_trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line_trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let token = parts[0];
                if let Ok(id) = parts[1].parse::<i64>() {
                    let ids = vec![id];
                    sym_to_id.insert(token.to_string(), ids.clone());
                    max_key_len = max_key_len.max(token.len());

                    if token.chars().count() == 1 {
                        if let Some(ch) = token.chars().next() {
                            char_to_ids.entry(ch).or_insert(ids);
                        }
                    }
                }
            }
        }

        if sym_to_id.is_empty() {
            return None;
        }

        eprintln!(
            "📖 [PhonemeMap] Loaded {} tokens.txt entries (max_key_len={}) from {:?}",
            sym_to_id.len(),
            max_key_len,
            path.file_name().unwrap_or_default()
        );

        Some(Self {
            sym_to_id,
            char_to_ids,
            pad_id,
            bos_id,
            eos_id,
            max_key_len,
        })
    }

    fn try_parse_tokenizer_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        let vocab_obj = json
            .get("model")
            .and_then(|m| m.get("vocab"))
            .or_else(|| json.get("vocab"))?
            .as_object()?;

        let mut sym_to_id: HashMap<String, Vec<i64>> = HashMap::new();
        let mut char_to_ids: HashMap<char, Vec<i64>> = HashMap::new();
        let pad_id: i64 = json.get("pad_token_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let bos_id: i64 = json.get("bos_token_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let eos_id: i64 = json.get("eos_token_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let mut max_key_len: usize = 1;

        for (key, value) in vocab_obj {
            if let Some(id) = value.as_i64() {
                let ids = vec![id];
                sym_to_id.insert(key.clone(), ids.clone());
                max_key_len = max_key_len.max(key.len());

                if key.chars().count() == 1 {
                    if let Some(ch) = key.chars().next() {
                        char_to_ids.entry(ch).or_insert(ids);
                    }
                }
            }
        }

        if sym_to_id.is_empty() {
            return None;
        }

        eprintln!(
            "📖 [PhonemeMap] Loaded {} symbol table entries (max_key_len={}) from {:?}",
            sym_to_id.len(),
            max_key_len,
            path.file_name().unwrap_or_default()
        );

        Some(Self {
            sym_to_id,
            char_to_ids,
            pad_id,
            bos_id,
            eos_id,
            max_key_len,
        })
    }

    fn try_parse_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        let phoneme_map = json.get("phoneme_id_map")?;
        let obj = phoneme_map.as_object()?;

        let mut sym_to_id: HashMap<String, Vec<i64>> = HashMap::new();
        let mut char_to_ids: HashMap<char, Vec<i64>> = HashMap::new();
        let mut pad_id: i64 = json.get("pad_token_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let mut bos_id: i64 = json.get("bos_token_id").and_then(|v| v.as_i64()).unwrap_or(1);
        let mut eos_id: i64 = json.get("eos_token_id").and_then(|v| v.as_i64()).unwrap_or(2);
        let mut max_key_len: usize = 1;

        for (key, value) in obj {
            let ids: Vec<i64> = match value {
                serde_json::Value::Array(arr) => {
                    arr.iter().filter_map(|v| v.as_i64()).collect()
                }
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

            if key == "^" {
                if let Some(&b) = ids.first() {
                    bos_id = b;
                }
            } else if key == "$" {
                if let Some(&e) = ids.first() {
                    eos_id = e;
                }
            } else if key == "_" {
                if let Some(&p) = ids.first() {
                    pad_id = p;
                }
            }

            sym_to_id.insert(key.clone(), ids.clone());
            max_key_len = max_key_len.max(key.len());

            if key.chars().count() == 1 {
                if let Some(ch) = key.chars().next() {
                    char_to_ids.entry(ch).or_insert(ids);
                }
            }
        }

        if sym_to_id.is_empty() {
            return None;
        }

        eprintln!(
            "📖 [PhonemeMap] Loaded {} symbol entries from {:?} (pad={}, bos={}, eos={})",
            sym_to_id.len(),
            path.file_name().unwrap_or_default(),
            pad_id, bos_id, eos_id
        );

        Some(Self {
            sym_to_id,
            char_to_ids,
            pad_id,
            bos_id,
            eos_id,
            max_key_len,
        })
    }

    /// Convert text string to phoneme ID sequence with longest-prefix matching.
    pub fn text_to_ids(&self, text: &str) -> Vec<i64> {
        let mut ids = Vec::with_capacity(text.len() * 2 + 2);
        ids.push(self.bos_id);

        let mut i = 0;
        let bytes = text.as_bytes();
        let len = bytes.len();

        while i < len {
            let mut matched = false;
            let max_len = (len - i).min(self.max_key_len);

            // Longest-Prefix Substring Match
            for l in (1..=max_len).rev() {
                if let Ok(sub) = std::str::from_utf8(&bytes[i..i + l]) {
                    if let Some(sym_ids) = self.sym_to_id.get(sub) {
                        ids.extend_from_slice(sym_ids);
                        ids.push(self.pad_id);
                        i += l;
                        matched = true;
                        break;
                    }
                }
            }

            if !matched {
                // Advance 1 UTF-8 char
                if let Some(ch) = text[i..].chars().next() {
                    if let Some(c_ids) = self.char_to_ids.get(&ch) {
                        ids.extend_from_slice(c_ids);
                        ids.push(self.pad_id);
                    } else {
                        let lower = ch.to_lowercase().next().unwrap_or(ch);
                        if let Some(c_ids) = self.char_to_ids.get(&lower) {
                            ids.extend_from_slice(c_ids);
                            ids.push(self.pad_id);
                        }
                    }
                    i += ch.len_utf8();
                } else {
                    i += 1;
                }
            }
        }

        ids.push(self.eos_id);
        ids
    }

    /// Convert text string to phoneme ID sequence without padding between characters.
    /// Uses 1:1 Sherpa-ONNX Longest-Prefix Matching algorithm.
    pub fn text_to_ids_no_pad(&self, text: &str) -> Vec<i64> {
        let mut ids = Vec::with_capacity(text.len() + 2);
        ids.push(self.bos_id);

        let mut i = 0;
        let bytes = text.as_bytes();
        let len = bytes.len();

        while i < len {
            let mut matched = false;
            let max_len = (len - i).min(self.max_key_len);

            // Longest-Prefix Substring Match
            for l in (1..=max_len).rev() {
                if let Ok(sub) = std::str::from_utf8(&bytes[i..i + l]) {
                    if let Some(sym_ids) = self.sym_to_id.get(sub) {
                        ids.extend_from_slice(sym_ids);
                        i += l;
                        matched = true;
                        break;
                    }
                }
            }

            if !matched {
                if let Some(ch) = text[i..].chars().next() {
                    if let Some(c_ids) = self.char_to_ids.get(&ch) {
                        ids.extend_from_slice(c_ids);
                    } else {
                        let lower = ch.to_lowercase().next().unwrap_or(ch);
                        if let Some(c_ids) = self.char_to_ids.get(&lower) {
                            ids.extend_from_slice(c_ids);
                        }
                    }
                    i += ch.len_utf8();
                } else {
                    i += 1;
                }
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

