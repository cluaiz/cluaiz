

use crate::audio::tts::family_adapter::TtsFamily;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

static GLOBAL_LEXICON: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Dynamically process and normalize text based on the TTS architecture family.
/// This prevents models that expect IPA (like Kokoro) from outputting gibberish 
/// when fed raw English characters.
pub fn process_text_for_family(text: &str, family: &TtsFamily, model_dir: &Path) -> String {
    // Attempt to load external dictionary once
    let _ = load_lexicon(model_dir);

    match family {
        TtsFamily::Kokoro => {
            // Kokoro requires IPA phonemes
            text_to_ipa(text)
        }
        TtsFamily::VitsPiper => {
            // Some Piper models expect Phonemes, some expect raw UTF-8. 
            // The phoneme_map usually handles VITS directly, but we do basic normalization here.
            text.to_lowercase()
        }
        _ => {
            // Default fallback for CosyVoiceMatcha, Audio8, etc.
            text.to_string()
        }
    }
}

/// Attempt to load `ipa_dict.json` if it exists in the model directory
fn load_lexicon(model_dir: &Path) {
    GLOBAL_LEXICON.get_or_init(|| {
        let dict_path = model_dir.join("voices").join("ipa_dict.json");
        let alt_path = model_dir.join("ipa_dict.json");

        let final_path = if dict_path.exists() {
            dict_path
        } else if alt_path.exists() {
            alt_path
        } else {
            return HashMap::new();
        };

        if let Ok(content) = fs::read_to_string(&final_path) {
            if let Ok(dict) = serde_json::from_str::<HashMap<String, String>>(&content) {
                tracing::info!("📖 [G2P Router] Loaded {} entries from {}", dict.len(), final_path.display());
                return dict;
            }
        }
        HashMap::new()
    });
}

/// Convert English text to approximate IPA (or exact if found in dictionary).
fn text_to_ipa(text: &str) -> String {
    let text = text.to_lowercase().replace(&['.', ',', '!', '?', ';', ':'][..], " ");
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut ipa_output = Vec::new();

    let dict = GLOBAL_LEXICON.get();

    for word in words {
        // 1. Check Lexicon
        if let Some(d) = dict {
            if let Some(ipa) = d.get(word) {
                ipa_output.push(ipa.clone());
                continue;
            }
        }

        // 2. Fallback Heuristic Rules for Kokoro (Approximation to prevent gibberish)
        let mut ipa_word = word.to_string();
        
        // Basic consonant digraphs
        ipa_word = ipa_word.replace("sh", "ʃ");
        ipa_word = ipa_word.replace("ch", "ʧ");
        ipa_word = ipa_word.replace("th", "θ"); // or ð
        ipa_word = ipa_word.replace("ph", "f");
        ipa_word = ipa_word.replace("ng", "ŋ");

        // Basic vowel approximations (extremely crude, just to satisfy Kokoro's token space)
        ipa_word = ipa_word.replace("ee", "iː");
        ipa_word = ipa_word.replace("oo", "uː");
        ipa_word = ipa_word.replace("ea", "iː");
        ipa_word = ipa_word.replace("ay", "eɪ");
        ipa_word = ipa_word.replace("ou", "aʊ");
        ipa_word = ipa_word.replace("ow", "oʊ");
        ipa_word = ipa_word.replace("aw", "ɔː");

        // Inject stress marker arbitrarily on the first vowel to satisfy Kokoro's prosody model 
        // (Without stress markers, Kokoro can output flat robotic or strange noises)
        if let Some(idx) = ipa_word.find(|c: char| "aeiouæɛɪɒʊʌəiːuːɔːɑːeɪaʊoʊ".contains(c)) {
            ipa_word.insert(idx, 'ˈ');
        }

        ipa_output.push(ipa_word);
    }

    ipa_output.join(" ")
}
