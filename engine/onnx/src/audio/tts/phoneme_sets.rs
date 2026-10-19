//! Dictionary IPA → the phoneme symbols a model was trained on.
//!
//! Pronunciation dictionaries write plain IPA ("ˈeɪ", "ɫ", stress at the syllable start), while
//! Kokoro speaks misaki symbols ("A" for eɪ) and Piper speaks espeak-ng IPA (stress on the vowel).
//! `data/phoneme_sets.json` describes each symbol set and how to recognise a model that uses it.

use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

const SETS_JSON: &str = include_str!("data/phoneme_sets.json");

#[derive(Deserialize)]
struct PhonemeSets {
    vowels: String,
    #[serde(default)]
    missing_symbol_fallback: Vec<(String, String)>,
    sets: Vec<PhonemeSet>,
}

#[derive(Deserialize)]
struct PhonemeSet {
    name: String,
    #[serde(default)]
    languages: Vec<String>,
    #[serde(rename = "match")]
    matcher: Match,
    replace: Vec<(String, String)>,
    #[serde(default)]
    stress_before_vowel: bool,
}

#[derive(Deserialize)]
struct Match {
    #[serde(default)]
    vocab_has_all: Vec<String>,
    #[serde(default)]
    config_phoneme_type: Option<String>,
}

const STRESS_MARKS: [char; 2] = ['ˈ', 'ˌ'];

fn sets() -> Option<&'static PhonemeSets> {
    static SETS: OnceLock<Option<PhonemeSets>> = OnceLock::new();
    SETS.get_or_init(|| match serde_json::from_str(SETS_JSON) {
        Ok(sets) => Some(sets),
        Err(e) => {
            tracing::error!("🔤 [PhonemeSets] phoneme_sets.json is invalid: {e}");
            None
        }
    })
    .as_ref()
}

/// Rewrites dictionary pronunciations into one model's symbol set.
pub struct Converter {
    name: &'static str,
    replace: &'static [(String, String)],
    stress_before_vowel: bool,
    vowels: HashSet<char>,
    /// Fallback rewrites for symbols this model has no token for
    missing: Vec<&'static (String, String)>,
}

impl Converter {
    pub fn name(&self) -> &str {
        self.name
    }

    pub fn convert(&self, ipa: &str) -> String {
        let replaced = self
            .replace
            .iter()
            .fold(ipa.to_owned(), |text, (from, to)| text.replace(from.as_str(), to));
        let replaced = self
            .missing
            .iter()
            .fold(replaced, |text, (from, to)| text.replace(from.as_str(), to));
        if self.stress_before_vowel {
            move_stress_to_vowel(&replaced, &self.vowels)
        } else {
            replaced
        }
    }
}

/// The converter for the model in `model_dir` speaking `lang`: its symbol set's rewrites (if one
/// matches) plus fallbacks for symbols it lacks. None when the model's symbols are unknown and no
/// set matches (IPA is then used as it is).
pub fn converter_for(model_dir: &Path, lang: &str) -> Option<Converter> {
    let sets = sets()?;
    let base = lang.split(['-', '_']).next().unwrap_or(lang).to_lowercase();
    let phoneme_type = config_phoneme_type(model_dir);
    let symbols = super::phoneme_map::PhonemeMap::from_model_dir(model_dir);
    let set = sets.sets.iter().find(|set| {
        let language_ok = set.languages.is_empty() || set.languages.iter().any(|l| *l == base);
        let type_ok = set
            .matcher
            .config_phoneme_type
            .as_ref()
            .map_or(true, |t| phoneme_type.as_deref() == Some(t.as_str()));
        let vocab_ok = set.matcher.vocab_has_all.is_empty()
            || symbols
                .as_ref()
                .is_some_and(|map| set.matcher.vocab_has_all.iter().all(|s| map.has_symbol(s)));
        language_ok && type_ok && vocab_ok
    });
    let missing: Vec<&'static (String, String)> = symbols
        .as_ref()
        .map(|map| {
            sets.missing_symbol_fallback
                .iter()
                .filter(|(from, _)| !map.has_symbol(from))
                .collect()
        })
        .unwrap_or_default();
    if set.is_none() && missing.is_empty() {
        return None;
    }
    Some(Converter {
        name: set.map_or("plain IPA", |s| s.name.as_str()),
        replace: set.map_or(&[], |s| s.replace.as_slice()),
        stress_before_vowel: set.is_some_and(|s| s.stress_before_vowel),
        vowels: sets.vowels.chars().collect(),
        missing,
    })
}

/// `phoneme_type` from the model's config files (Piper writes "espeak" there).
fn config_phoneme_type(model_dir: &Path) -> Option<String> {
    std::fs::read_dir(model_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.ends_with(".json") && !name.starts_with("tokenizer")
        })
        .find_map(|p| {
            let json: serde_json::Value = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
            json.get("phoneme_type")?.as_str().map(str::to_owned)
        })
}

/// "əˈdɛlfə" → "ədˈɛlfə": a stress mark waits until the next vowel of its syllable.
fn move_stress_to_vowel(ipa: &str, vowels: &HashSet<char>) -> String {
    let mut out = String::with_capacity(ipa.len());
    let mut pending: Option<char> = None;
    for c in ipa.chars() {
        if STRESS_MARKS.contains(&c) {
            pending = Some(c);
            continue;
        }
        if c.is_whitespace() {
            pending = None;
        } else if vowels.contains(&c) {
            if let Some(mark) = pending.take() {
                out.push(mark);
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn converter(name: &str) -> Converter {
        let sets = sets().unwrap();
        let set = sets.sets.iter().find(|s| s.name == name).unwrap();
        Converter {
            name: &set.name,
            replace: &set.replace,
            stress_before_vowel: set.stress_before_vowel,
            vowels: sets.vowels.chars().collect(),
            missing: Vec::new(),
        }
    }

    #[test]
    fn misaki_uses_its_diphthong_letters_and_vowel_stress() {
        let misaki = converter("misaki");
        assert_eq!(misaki.convert("həˈɫoʊ"), "həlˈO");
        assert_eq!(misaki.convert("ˈbɝd"), "bˈɜɹd");
        assert_eq!(misaki.convert("ˈdʒʌdʒ"), "ʤˈʌʤ");
    }

    #[test]
    fn espeak_keeps_ipa_but_moves_stress_and_drops_dark_l() {
        let espeak = converter("espeak");
        assert_eq!(espeak.convert("əˈdɛɫfə"), "ədˈɛlfə");
        assert_eq!(espeak.convert("ˈbɝd"), "bˈɜːd");
    }

    #[test]
    fn test_missing_tone_sequences_keep_long_contours_and_glottal_stops() {
        let data: serde_json::Value = serde_json::from_str(SETS_JSON).unwrap();
        let mut actual = converter("espeak");
        actual.missing = sets().unwrap().missing_symbol_fallback.iter().collect();
        for pair in data["fallback_examples"].as_array().unwrap() {
            assert_eq!(actual.convert(pair[0].as_str().unwrap()), pair[1].as_str().unwrap());
        }
    }
}
