//! Languages written in Latin letters (Hindi as Hinglish: "Main aapki kaise madad kar sakta hoon?")
//! are rewritten into their own script before G2P, so their rules and voice say them instead of
//! English rules. One JSON file per language in `data/transliteration/` (embedded at build time).
//!
//! Decided per sentence: a sentence is the language when enough of its Latin words are known words
//! of it compared with dictionary English words. In such a sentence known and ambiguous words are
//! replaced, real English words stay English ("Large Language Model"), and other words are written
//! with the file's letter rules. Sentences that do not qualify are left as they are.

use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Romanization {
    language: String,
    /// Language whose (downloaded) dictionary tells real English words apart
    english_dictionary: String,
    min_words: usize,
    min_share: f32,
    words: HashMap<String, String>,
    /// Words that are also English: replaced in a qualifying sentence, never counted as evidence
    #[serde(default)]
    ambiguous: HashMap<String, String>,
    letters: Letters,
    #[serde(default)]
    #[cfg_attr(not(test), allow(dead_code))]
    examples: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Letters {
    consonants: HashMap<String, String>,
    vowels: HashMap<String, String>,
    vowel_signs: HashMap<String, String>,
    #[serde(default)]
    final_vowel_signs: HashMap<String, String>,
    virama: String,
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
pub fn to_native_script(text: &str) -> Cow<'_, str> {
    if romanizations().is_empty() || !text.chars().any(|c| c.is_ascii_alphabetic()) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut changed = false;
    for sentence in text.split_inclusive(&SENTENCE_ENDS[..]) {
        match convert_sentence(sentence) {
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

/// Latin-letter words of a sentence with their byte ranges.
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

fn convert_sentence(sentence: &str) -> Option<String> {
    let words = latin_words(sentence);
    if words.is_empty() {
        return None;
    }
    let lower: Vec<String> = words.iter().map(|&(s, e)| sentence[s..e].to_lowercase()).collect();
    // The language with the most known words that passes its thresholds
    let (rom, english) = romanizations()
        .iter()
        .filter_map(|rom| {
            let english = super::ipa_dictionary::cached_language_lexicon(&rom.english_dictionary);
            let known = lower.iter().filter(|w| rom.words.contains_key(*w)).count();
            let is_english = |w: &String| {
                !rom.words.contains_key(w)
                    && !rom.ambiguous.contains_key(w)
                    && english.as_ref().is_some_and(|d| d.contains_key(w))
            };
            let english_words = lower.iter().filter(|w| is_english(w)).count();
            let share = known as f32 / (known + english_words).max(1) as f32;
            (known >= rom.min_words && share >= rom.min_share).then_some((known, rom, english))
        })
        .max_by_key(|(known, _, _)| *known)
        .map(|(_, rom, english)| (rom, english))?;

    let mut out = String::with_capacity(sentence.len() * 2);
    let mut last = 0;
    for (&(s, e), word) in words.iter().zip(&lower) {
        out.push_str(&sentence[last..s]);
        let native = rom
            .words
            .get(word)
            .or_else(|| rom.ambiguous.get(word))
            .cloned()
            .or_else(|| {
                let english_word = english.as_ref().is_some_and(|d| d.contains_key(word));
                (!english_word).then(|| rom.letters.write(word))
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

impl Letters {
    /// A lowercase ASCII word in the native script: consonant + vowel sign, virama between
    /// consonants, independent vowels at the start or after a vowel.
    fn write(&self, word: &str) -> String {
        let mut out = String::new();
        let mut i = 0;
        let mut after_consonant = false;
        while i < word.len() {
            let rest = &word[i..];
            if after_consonant {
                if let Some((n, sign)) = longest(&self.vowel_signs, rest) {
                    let at_end = i + n == word.len();
                    let sign = at_end
                        .then(|| self.final_vowel_signs.get(&rest[..n]))
                        .flatten()
                        .map_or(sign, String::as_str);
                    out.push_str(sign);
                    i += n;
                    after_consonant = false;
                    continue;
                }
            }
            if let Some((n, consonant)) = longest(&self.consonants, rest) {
                if after_consonant {
                    out.push_str(&self.virama);
                }
                out.push_str(consonant);
                i += n;
                after_consonant = true;
            } else if let Some((n, vowel)) = longest(&self.vowels, rest) {
                out.push_str(vowel);
                i += n;
                after_consonant = false;
            } else {
                i += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file's examples hold (sentence → expected text), checked without an English
    /// dictionary on disk.
    #[test]
    fn every_romanization_file_converts_its_examples() {
        let mut checked = 0;
        for rom in romanizations() {
            for (input, expected) in &rom.examples {
                assert_eq!(to_native_script(input), expected.as_str(), "{} {input}", rom.language);
                checked += 1;
            }
        }
        assert!(checked >= 4, "only {checked} examples");
    }

    #[test]
    fn letters_write_hindi_from_latin() {
        let hi = romanizations().iter().find(|r| r.language == "hi").unwrap();
        assert_eq!(hi.letters.write("sakta"), "सक्ता");
        assert_eq!(hi.letters.write("karni"), "कर्नी");
        assert_eq!(hi.letters.write("aapki"), "आप्की");
    }
}
