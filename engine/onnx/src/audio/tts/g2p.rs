use crate::audio::tts::family_adapter::TtsFamily;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::RwLock;

static DYNAMIC_LEXICON: RwLock<Option<HashMap<String, String>>> = RwLock::new(None);
static CURRENT_LOADED_MODEL_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Dynamically process and normalize text for any TTS architecture family
/// using model directory manifest assets (`lexicon.txt`, `dict_*.json`).
///
/// Enforces 100% Zero-Hardcoding Law: Dynamic lexicon files are read dynamically from disk.
/// All 8 TTS families and 300+ languages execute through this single universal pipeline.
pub fn process_text_for_family(text: &str, _family: &TtsFamily, model_dir: &Path) -> String {
    load_lexicon(model_dir);
    lexicon_text_to_ipa(text, model_dir)
}

/// Dynamically resolve model language code from config.json or any sibling JSON manifest file
pub(crate) fn get_model_language(model_dir: &Path) -> String {
    if let Ok(entries) = fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if (ext == "json" || name.ends_with(".onnx.json"))
                    && name != "tokenizer.json"
                    && name != "tokenizer_config.json"
                    && name != "voices.json"
                    && name != "hf_metadata.json"
                {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            // 1. Priority 1: Direct espeak voice tag (e.g. "hi", "en-us", "fr-fr", "de", "es", "ar")
                            if let Some(voice) =
                                json.pointer("/espeak/voice").and_then(|v| v.as_str())
                            {
                                let v = voice.trim();
                                if !v.is_empty() {
                                    return v.to_lowercase();
                                }
                            }
                            // 2. Priority 2: Language code (e.g. "hi_IN", "en_US", "fr_FR", "de_DE", "zh_CN")
                            if let Some(code) =
                                json.pointer("/language/code").and_then(|v| v.as_str())
                            {
                                let c = code.trim();
                                if !c.is_empty() {
                                    return c.to_lowercase().replace('_', "-");
                                }
                            }
                            // 3. Priority 3: Language family/name (e.g. "hi", "en", "fr")
                            if let Some(family) =
                                json.pointer("/language/family").and_then(|v| v.as_str())
                            {
                                let f = family.trim();
                                if !f.is_empty() {
                                    return f.to_lowercase();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Dynamic Fallback: Extract 2-letter ISO language prefix from folder name if present
    let folder_name = model_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    for part in folder_name.split(&['-', '_', ' '][..]) {
        if part.len() == 2 && part.chars().all(|c| c.is_ascii_lowercase()) {
            return part.to_string();
        }
    }

    "en-us".to_string()
}

/// Dynamically load dictionary/lexicon files from model directory assets.
/// Scans for `lexicon.txt`, `lexicon_en.txt`, `dict_en.json`, `ipa_dict.json`, etc.
fn load_lexicon(model_dir: &Path) {
    if let Ok(guard) = CURRENT_LOADED_MODEL_DIR.read() {
        if let Some(ref current_dir) = *guard {
            if current_dir == model_dir {
                return;
            }
        }
    }

    let candidates = [
        model_dir.join("lexicon.txt"),
        model_dir.join("lexicon_en.txt"),
        model_dir.join("dict_en.json"),
        model_dir.join("voices").join("ipa_dict.json"),
        model_dir.join("ipa_dict.json"),
        model_dir.join("dict_hi.json"),
        model_dir.join("dict_es.json"),
        model_dir.join("dict_fr.json"),
    ];

    let mut combined_dict = HashMap::new();

    // 1. Parse phoneme_map metadata directly from model config JSON files
    if let Ok(entries) = fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "json" || name.ends_with(".onnx.json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(pmap) = val.get("phoneme_map").and_then(|v| v.as_object()) {
                                let mut count = 0;
                                for (k, v) in pmap {
                                    if let Some(target) = v.as_str() {
                                        combined_dict.insert(k.to_lowercase(), target.to_string());
                                        count += 1;
                                    } else if let Some(arr) = v.as_array() {
                                        let items: Vec<String> = arr
                                            .iter()
                                            .filter_map(|i| i.as_str().map(|s| s.to_string()))
                                            .collect();
                                        if !items.is_empty() {
                                            combined_dict.insert(k.to_lowercase(), items.join(" "));
                                            count += 1;
                                        }
                                    }
                                }
                                if count > 0 {
                                    eprintln!(
                                        "📖 [G2P Router] Loaded {} phoneme_map entries from header {:?}",
                                        count,
                                        path.file_name().unwrap_or_default()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Parse candidate lexicon files (lexicon.txt, dict_en.json, ipa_dict.json)
    for path in &candidates {
        if !path.exists() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if ext == "json" {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(dict) = serde_json::from_str::<HashMap<String, String>>(&content) {
                    eprintln!(
                        "📖 [G2P Router] Loaded {} dynamic JSON lexicon entries from {:?}",
                        dict.len(),
                        path.file_name().unwrap_or_default()
                    );
                    combined_dict.extend(dict);
                }
            }
        } else if ext == "txt" {
            if let Ok(content) = fs::read_to_string(path) {
                let mut count = 0;
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let word = parts[0].to_lowercase();
                        let ipa = parts[1..].join(" ");
                        combined_dict.insert(word, ipa);
                        count += 1;
                    }
                }
                eprintln!(
                    "📖 [G2P Router] Loaded {} dynamic lexicon.txt entries from {:?}",
                    count,
                    path.file_name().unwrap_or_default()
                );
            }
        }
    }

    // 3. Load dynamic compiled lexicon (from assets/ipa_dictionary)
    let dict = super::ipa_dictionary::load_or_compile_lexicon(model_dir);
    combined_dict.extend(dict);

    if let Ok(mut guard) = DYNAMIC_LEXICON.write() {
        *guard = Some(combined_dict);
    }
    if let Ok(mut guard) = CURRENT_LOADED_MODEL_DIR.write() {
        *guard = Some(model_dir.to_path_buf());
    }
}

/// Convert input text using dynamic lexicon assets or clean word fallback.
fn lexicon_text_to_ipa(text: &str, _model_dir: &Path) -> String {
    let clean_text = text.to_lowercase();
    let words: Vec<&str> = clean_text.split_whitespace().collect();

    let lexicon_guard = DYNAMIC_LEXICON.read().ok();
    let dict = lexicon_guard.as_ref().and_then(|g| g.as_ref());

    let mut output_phonemes = Vec::new();

    for word in words {
        let clean_word = word.trim_matches(|c: char| {
            c.is_ascii_punctuation() || ('\u{2000}'..='\u{206F}').contains(&c)
        });
        if clean_word.is_empty() {
            output_phonemes.push(word.to_string());
            continue;
        }

        let start = clean_word.as_ptr() as usize - word.as_ptr() as usize;
        let end = start + clean_word.len();
        let leading = &word[..start];
        let trailing = &word[end..];

        // 1. Try lexicon lookup for the clean word
        let mut ipa = clean_word.to_string();
        let mut matched = false;
        if let Some(d) = dict {
            if let Some(lookup_ipa) = d.get(clean_word) {
                ipa = lookup_ipa.clone();
                matched = true;
            }
        }
        eprintln!(
            "DEBUG: word='{}', clean='{}', matched={}, ipa='{}'",
            word, clean_word, matched, ipa
        );

        // 2. Re-attach punctuation
        let mut reconstructed = String::new();
        if !leading.is_empty() {
            reconstructed.push_str(leading);
        }
        reconstructed.push_str(&ipa);
        if !trailing.is_empty() {
            reconstructed.push_str(trailing);
        }
        output_phonemes.push(reconstructed);
    }

    if output_phonemes.is_empty() {
        clean_text
    } else {
        output_phonemes.join(" ")
    }
}

/// Convert input text into token IDs using dynamic manifest symbol maps.
pub fn text_to_token_ids_fallback(
    text: &str,
    manifest: &crate::audio::tts::TtsModelManifest,
) -> Vec<i64> {
    let mut ids = Vec::new();

    if let Some(ref pmap) = manifest.phoneme_id_map {
        for ch in text.chars() {
            let s = ch.to_string();
            if let Some(&id) = pmap.get(&s) {
                ids.push(id);
            } else if let Some(&id) = pmap.get(&s.to_lowercase()) {
                ids.push(id);
            }
        }
    } else if let Some(ref tmap) = manifest.tokens_map {
        for ch in text.chars() {
            let s = ch.to_string();
            if let Some(&id) = tmap.get(&s) {
                ids.push(id);
            } else if let Some(&id) = tmap.get(&s.to_lowercase()) {
                ids.push(id);
            }
        }
    }

    if ids.is_empty() {
        ids = text.chars().map(|c| c as i64).collect();
    }

    ids
}
