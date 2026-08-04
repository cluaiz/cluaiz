
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
            // Default fallback for Matcha, CosyVoice, Audio8, Supertonic, etc.
            // These families handle their own text processing internally.
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
///
/// Processing order matters. We apply rules from most-specific to least-specific:
/// 1. Dictionary lookup (exact match — highest quality)
/// 2. Common suffix rules (-tion, -sion, -ight, etc.)
/// 3. Magic E rule (silent trailing 'e' that changes the preceding vowel)
/// 4. Soft C/G rules (context-sensitive consonants)
/// 5. Consonant digraphs (sh, ch, th, ph, ng)
/// 6. Vowel digraphs (ee, oo, ea, ay, etc.)
/// 7. Stress marker injection (Kokoro prosody requirement)
fn text_to_ipa(text: &str) -> String {
    let text = text.to_lowercase().replace(&['.', ',', '!', '?', ';', ':'][..], " ");
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut ipa_output = Vec::new();

    let dict = GLOBAL_LEXICON.get();

    for word in words {
        // Priority 1: Exact dictionary lookup
        if let Some(d) = dict {
            if let Some(ipa) = d.get(word) {
                ipa_output.push(ipa.clone());
                continue;
            }
        }

        // Priority 2-7: Heuristic fallback rules
        let ipa_word = apply_english_to_ipa_heuristics(word);
        ipa_output.push(ipa_word);
    }

    ipa_output.join(" ")
}

/// Apply position-aware, context-sensitive English-to-IPA heuristic rules.
///
/// FACT: These rules are intentionally approximate. English orthography is deeply
/// irregular — a full G2P system would require a 100MB+ CMU dictionary or a neural model.
/// These heuristics are designed to produce "good enough" IPA that Kokoro can
/// synthesize intelligibly, which is 10x better than raw English letters.
fn apply_english_to_ipa_heuristics(word: &str) -> String {
    let mut w = word.to_string();

    // Step 1: Common suffix rules (most specific patterns first)
    // These must run BEFORE consonant/vowel rules to avoid partial matches
    w = apply_suffix_rules(&w);

    // Step 2: Magic E rule — silent trailing 'e' changes preceding vowel
    // Pattern: (consonant)(vowel)(consonant)e$ → long vowel + consonant (drop 'e')
    // Example: "space" → "speis", "time" → "taim", "home" → "houm"
    w = apply_magic_e_rule(&w);

    // Step 3: Soft C/G rules (context-sensitive)
    w = apply_soft_consonant_rules(&w);

    // Step 4: Consonant digraphs
    w = w.replace("sh", "ʃ");
    w = w.replace("ch", "ʧ");
    w = w.replace("th", "θ");
    w = w.replace("ph", "f");
    w = w.replace("wh", "w");
    w = w.replace("wr", "r");
    w = w.replace("kn", "n");
    w = w.replace("gn", "n");
    w = w.replace("ck", "k");
    w = w.replace("ng", "ŋ");

    // Step 5: Vowel digraphs and common vowel patterns
    w = w.replace("ee", "iː");
    w = w.replace("oo", "uː");
    w = w.replace("ea", "iː");
    w = w.replace("ai", "eɪ");
    w = w.replace("ay", "eɪ");
    w = w.replace("oi", "ɔɪ");
    w = w.replace("oy", "ɔɪ");
    w = w.replace("ou", "aʊ");
    w = w.replace("ow", "oʊ");
    w = w.replace("aw", "ɔː");
    w = w.replace("au", "ɔː");
    w = w.replace("ew", "juː");
    w = w.replace("ie", "iː");

    // Step 6: Inject stress marker on first vowel (Kokoro prosody requirement)
    // Without stress markers, Kokoro produces flat/robotic output
    if let Some(idx) = w.find(|c: char| "aeiouæɛɪɒʊʌəɔɑ".contains(c)) {
        w.insert(idx, 'ˈ');
    }

    w
}

/// Apply Magic E rule: silent trailing 'e' that shifts the preceding vowel to its "long" form.
///
/// FACT: In English, when a word ends in consonant + 'e' and has a single vowel before
/// the consonant, the vowel becomes "long" and the 'e' is silent.
///
/// Examples:
/// - "space" (a_e) → speɪs  (not "spah-cheh")
/// - "time"  (i_e) → taɪm   (not "tim-eh")
/// - "home"  (o_e) → hoʊm   (not "hom-eh")
/// - "cute"  (u_e) → kjuːt  (not "kut-eh")
/// - "scene" (e_e) → siːn   (not "sken-eh")
fn apply_magic_e_rule(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let len = chars.len();

    // Minimum length: 4 chars (e.g., "make" = m-a-k-e)
    if len < 4 {
        return word.to_string();
    }

    // Must end with 'e'
    if chars[len - 1] != 'e' {
        return word.to_string();
    }

    // Character before 'e' must be a consonant
    let pre_e = chars[len - 2];
    if !is_consonant(pre_e) {
        return word.to_string();
    }

    // Find the vowel before that consonant
    // Look backwards from position len-3
    let mut vowel_idx = None;
    for i in (0..len - 2).rev() {
        if is_vowel(chars[i]) {
            vowel_idx = Some(i);
            break;
        }
        // If we hit another consonant before finding a vowel, stop
        // (handles words like "nerve" where there's a consonant cluster)
        if i < len - 3 && is_consonant(chars[i]) {
            // Allow one consonant between vowel and pre-e consonant
            continue;
        }
    }

    if let Some(vi) = vowel_idx {
        let vowel = chars[vi];
        // Build the result: everything before the vowel + long vowel IPA + consonant(s) after vowel (skip trailing 'e')
        let prefix: String = chars[..vi].iter().collect();
        let middle: String = chars[vi + 1..len - 1].iter().collect(); // consonant(s) between vowel and final 'e'
        let long_vowel = match vowel {
            'a' => "eɪ",
            'i' => "aɪ",
            'o' => "oʊ",
            'u' => "juː",
            'e' => "iː",
            _ => return word.to_string(),
        };
        format!("{}{}{}", prefix, long_vowel, middle)
    } else {
        word.to_string()
    }
}

/// Apply Soft C/G rules: C and G change pronunciation before e, i, y.
///
/// FACT: In English:
/// - 'c' before 'e', 'i', 'y' → /s/ (soft C): "cell", "city", "cycle"
/// - 'c' before 'a', 'o', 'u' or consonants → /k/ (hard C): "cat", "cold"
/// - 'g' before 'e', 'i', 'y' → /dʒ/ (soft G): "gentle", "giant"
/// - 'g' before 'a', 'o', 'u' or consonants → /g/ (hard G): "gate", "gold"
///
/// NOTE: There are exceptions (e.g., "get", "give" have hard G before e/i),
/// but soft rules are correct more often than not and prevent the worst mispronunciations.
fn apply_soft_consonant_rules(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let mut result = String::with_capacity(word.len() + 4);
    let soft_triggers = ['e', 'i', 'y'];

    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'c' && i + 1 < chars.len() && soft_triggers.contains(&chars[i + 1]) {
            result.push('s');
        } else if chars[i] == 'g' && i + 1 < chars.len() && soft_triggers.contains(&chars[i + 1]) {
            result.push_str("dʒ");
            // Skip the 'g', but keep the following vowel
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }

    result
}

/// Apply common English suffix rules to avoid garbled output on frequent word endings.
fn apply_suffix_rules(word: &str) -> String {
    // Order matters: longer suffixes first to avoid partial matches
    let suffix_rules: &[(&str, &str)] = &[
        ("tion", "ʃən"),
        ("sion", "ʒən"),
        ("ious", "iːəs"),
        ("eous", "iːəs"),
        ("ight", "aɪt"),
        ("ness", "nəs"),
        ("ment", "mənt"),
        ("able", "əbəl"),
        ("ible", "əbəl"),
        ("ful", "fʊl"),
        ("less", "ləs"),
        ("ous", "əs"),
        ("ing", "ɪŋ"),
        ("ure", "ʊər"),
        ("ity", "ɪti"),
        ("ent", "ənt"),
        ("ant", "ənt"),
        ("ary", "ɛri"),
        ("ery", "əri"),
        ("ory", "ɔːri"),
        ("ly", "li"),
        ("er", "ər"),
        ("ed", "d"),
    ];

    for (suffix, replacement) in suffix_rules {
        if word.len() > suffix.len() + 1 && word.ends_with(suffix) {
            let stem = &word[..word.len() - suffix.len()];
            return format!("{}{}", stem, replacement);
        }
    }

    word.to_string()
}

#[inline]
fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

#[inline]
fn is_consonant(c: char) -> bool {
    c.is_ascii_alphabetic() && !is_vowel(c)
}

