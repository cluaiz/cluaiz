//! Languages written in English letters (Hindi as Hinglish: "Main aapki kaise madad kar sakta hoon?")
//! are rewritten into their own script before G2P, so their rules and voice say them instead of
//! English rules. One JSON file per language in `data/transliteration/` (embedded at build time).
//!
//! Words are written with the language's standard letter chart (Hindi: the varnamala as school
//! charts write it in English letters, a अ, aa आ, kh ख, chh छ ...); `spellings` lists only the words
//! that people write differently from the chart (hamara → हमारा, theek → ठीक).
//!
//! Decided per sentence: known words count fully and words that are not English partly; when that
//! evidence is large enough next to the sentence's English words, its Hindi words are rewritten and
//! real English words stay English ("Large Language Model"). Other sentences are left as they are.

use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Romanization {
    language: String,
    /// Wins a tie when no file is the model's language (romanized words several languages share)
    #[serde(default)]
    default: bool,
    /// Language whose (downloaded) dictionary tells real English words apart
    english_dictionary: String,
    min_evidence: f32,
    min_share: f32,
    /// Evidence of a word that is neither known nor English (needs the English dictionary)
    unknown_word_weight: f32,
    /// Common words of the language that the chart writes correctly
    #[serde(default)]
    known_words: HashSet<String>,
    /// Common words written differently from the chart
    #[serde(default)]
    spellings: HashMap<String, String>,
    /// Words that are also English: replaced in a qualifying sentence, never counted as evidence
    #[serde(default)]
    ambiguous: HashMap<String, String>,
    chart: Chart,
    #[serde(default)]
    #[cfg_attr(not(test), allow(dead_code))]
    examples: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Chart {
    consonants: HashMap<String, String>,
    vowels: HashMap<String, String>,
    vowel_signs: HashMap<String, String>,
    /// Vowel signs for the rest of a word when it ends with them ("sakta": final a is आ; Urdu
    /// "kartay": final ay is े)
    #[serde(default)]
    final_vowel_signs: HashMap<String, String>,
    /// A consonant ending the word gets a virama (scripts whose rules say every inherent vowel)
    #[serde(default)]
    final_consonant_virama: bool,
    #[serde(default)]
    anusvara: Option<Anusvara>,
    /// Write a virama only in clusters at the start or end of a word; inside a word the native
    /// rules decide which inherent vowels are said ("aapka" → आपका, said aːpkaː)
    #[serde(default)]
    cluster_only_at_word_edges: bool,
    virama: String,
}

/// "n" after a vowel and before these consonants is the nasal mark (hindi → हिंदी)
#[derive(Deserialize)]
struct Anusvara {
    letter: String,
    mark: String,
    before: Vec<String>,
}

/// Characters that end a sentence for the decision
const SENTENCE_ENDS: [char; 5] = ['.', '!', '?', '\n', '।'];

fn romanizations() -> &'static [Romanization] {
    static ALL: OnceLock<Vec<Romanization>> = OnceLock::new();
    ALL.get_or_init(|| {
        super::rules_g2p::TRANSLITERATION_FILES
            .iter()
            .filter_map(|(name, text)| match serde_json::from_str(text) {
                Ok(r) => Some(r),
                Err(e) => {
                    tracing::error!("🔤 [Romanized] {name} is invalid: {e}");
                    None
                }
            })
            .collect()
    })
}

/// Text with every romanized sentence of a known language written in that language's script.
/// `model_language` decides between languages whose romanized words look alike.
pub fn to_native_script<'t>(text: &'t str, model_language: &str) -> Cow<'t, str> {
    if romanizations().is_empty() || !text.chars().any(|c| c.is_ascii_alphabetic()) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut changed = false;
    for sentence in text.split_inclusive(&SENTENCE_ENDS[..]) {
        match convert_sentence(sentence, model_language) {
            Some(native) => {
                out.push_str(&native);
                changed = true;
            }
            None => out.push_str(sentence),
        }
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// English-letter words of a sentence with their byte ranges.
fn latin_words(sentence: &str) -> Vec<(usize, usize)> {
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in sentence.char_indices().chain(std::iter::once((sentence.len(), ' '))) {
        match (c.is_ascii_alphabetic(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    words
}

fn convert_sentence(sentence: &str, model_language: &str) -> Option<String> {
    let words = latin_words(sentence);
    if words.is_empty() {
        return None;
    }
    let lower: Vec<String> = words.iter().map(|&(s, e)| sentence[s..e].to_lowercase()).collect();
    let model_base = model_language.split(['-', '_']).next().unwrap_or(model_language).to_lowercase();
    // The language with the most evidence that passes its thresholds; a tie goes to the model's
    // language, then to the default file
    let (rom, english) = romanizations()
        .iter()
        .filter_map(|rom| {
            let english = super::ipa_dictionary::cached_language_lexicon(&rom.english_dictionary);
            let in_english = |w: &String| english.as_ref().is_some_and(|d| d.contains_key(w));
            let (mut evidence, mut english_words) = (0.0_f32, 0.0_f32);
            for w in &lower {
                if rom.is_known(w) {
                    evidence += 1.0;
                } else if rom.ambiguous.contains_key(w)
                    || romanizations().iter().any(|other| other.is_known(w))
                {
                    // Also English, or another language's word: evidence for neither
                    continue;
                } else if in_english(w) {
                    english_words += 1.0;
                } else if english.is_some() {
                    evidence += rom.unknown_word_weight;
                }
            }
            let share = evidence / (evidence + english_words).max(f32::EPSILON);
            (evidence >= rom.min_evidence && share >= rom.min_share).then_some((evidence, rom, english))
        })
        .max_by(|a, b| {
            let rank = |rom: &Romanization| (rom.language == model_base, rom.default);
            a.0.total_cmp(&b.0).then_with(|| rank(a.1).cmp(&rank(b.1)))
        })
        .map(|(_, rom, english)| (rom, english))?;

    let mut out = String::with_capacity(sentence.len() * 2);
    let mut last = 0;
    for (&(s, e), word) in words.iter().zip(&lower) {
        out.push_str(&sentence[last..s]);
        let native = rom
            .spellings
            .get(word)
            .or_else(|| rom.ambiguous.get(word))
            .cloned()
            .or_else(|| {
                let english_word = english.as_ref().is_some_and(|d| d.contains_key(word));
                (rom.known_words.contains(word) || !english_word).then(|| rom.chart.write(word))
            });
        out.push_str(native.as_deref().unwrap_or(&sentence[s..e]));
        last = e;
    }
    out.push_str(&sentence[last..]);
    tracing::debug!("🔤 [Romanized] A sentence read as '{}' ({} words)", rom.language, words.len());
    Some(out)
}

/// Longest key of `map` at the start of `rest` (keys are ASCII).
fn longest<'m>(map: &'m HashMap<String, String>, rest: &str) -> Option<(usize, &'m str)> {
    (1..=rest.len().min(4))
        .rev()
        .find_map(|n| map.get(rest.get(..n)?).map(|v| (n, v.as_str())))
}

impl Romanization {
    fn is_known(&self, word: &str) -> bool {
        self.known_words.contains(word) || self.spellings.contains_key(word)
    }
}

impl Chart {
    /// A lowercase ASCII word in the native script by the chart: consonant + vowel sign, nasal mark
    /// for "n" before a stop, virama in clusters, independent vowels at the start or after a vowel.
    fn write(&self, word: &str) -> String {
        let mut out = String::new();
        let mut i = 0;
        let (mut after_consonant, mut after_vowel, mut seen_vowel) = (false, false, false);
        while i < word.len() {
            let rest = &word[i..];
            if after_consonant {
                if let Some(sign) = self.final_vowel_signs.get(rest) {
                    out.push_str(sign);
                    break;
                }
                if let Some((n, sign)) = longest(&self.vowel_signs, rest) {
                    out.push_str(sign);
                    i += n;
                    (after_consonant, after_vowel, seen_vowel) = (false, true, true);
                    continue;
                }
            }
            if let Some(an) = self.anusvara.as_ref().filter(|_| after_vowel) {
                let next = rest
                    .strip_prefix(an.letter.as_str())
                    .and_then(|after| longest(&self.consonants, after).map(|(n, _)| &after[..n]));
                if next.is_some_and(|c| an.before.iter().any(|b| b == c)) {
                    out.push_str(&an.mark);
                    i += an.letter.len();
                    (after_consonant, after_vowel) = (false, false);
                    continue;
                }
            }
            if let Some((n, consonant)) = longest(&self.consonants, rest) {
                let edge = !seen_vowel || i + n == word.len();
                if after_consonant && (edge || !self.cluster_only_at_word_edges) {
                    out.push_str(&self.virama);
                }
                out.push_str(consonant);
                i += n;
                (after_consonant, after_vowel) = (true, false);
            } else if let Some((n, vowel)) = longest(&self.vowels, rest) {
                out.push_str(vowel);
                i += n;
                (after_consonant, after_vowel, seen_vowel) = (false, true, true);
            } else {
                i += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
        if after_consonant && self.final_consonant_virama {
            out.push_str(&self.virama);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file's examples hold (sentence → expected text) for a model speaking that language.
    #[test]
    fn every_romanization_file_converts_its_examples() {
        let mut checked = 0;
        for rom in romanizations() {
            for (input, expected) in &rom.examples {
                let got = to_native_script(input, &rom.language);
                assert_eq!(got, expected.as_str(), "{} {input}", rom.language);
                checked += 1;
            }
        }
        assert!(checked >= 4, "only {checked} examples");
    }

    /// For an English model, the words of the sentence pick the language
    #[test]
    fn words_pick_the_language_for_an_english_model() {
        let script = |text: &str| super::super::languages::script_language(&to_native_script(text, "en-us"))
            .map(str::to_owned);
        let contains = |text: &str, from: u32, to: u32| {
            to_native_script(text, "en-us").chars().any(|c| (from..=to).contains(&(c as u32)))
        };
        assert!(contains("Sat sri akal ji, ki haal hai?", 0x0A00, 0x0A7F), "Punjabi → Gurmukhi");
        assert!(contains("Vanakkam, eppadi irukeenga?", 0x0B80, 0x0BFF), "Tamil");
        assert!(contains("Kem cho? Majama", 0x0A80, 0x0AFF), "Gujarati");
        assert!(contains("Tumi kemon acho?", 0x0980, 0x09FF), "Bengali");
        assert_eq!(to_native_script("Tu kasa aahes?", "en-us"), "तू कसा आहेस?");
        assert_eq!(script("Aap kaise hain?").as_deref(), Some("hi"));
        assert_eq!(to_native_script("Hello, how are you?", "en-us"), "Hello, how are you?");
    }

    /// `spellings` holds only what the chart gets wrong, so the chart stays the rule
    #[test]
    fn spellings_list_only_words_the_chart_writes_differently() {
        for rom in romanizations() {
            for (word, native) in &rom.spellings {
                assert_ne!(rom.chart.write(word), *native, "{word}: the chart already writes it");
            }
        }
    }

    #[test]
    fn the_chart_writes_hindi_from_english_letters() {
        let hi = romanizations().iter().find(|r| r.language == "hi").unwrap();
        assert_eq!(hi.chart.write("sakta"), "सकता");
        assert_eq!(hi.chart.write("aapki"), "आपकी");
        assert_eq!(hi.chart.write("dost"), "दोस्त");
        assert_eq!(hi.chart.write("kya"), "क्या");
        assert_eq!(hi.chart.write("hindi"), "हिंदी");
        assert_eq!(hi.chart.write("gyaan"), "ज्ञान");
    }
}
