//! Text analysis shared by the tool matcher and the chat memory: whole words (split on case,
//! `_` and `-` too), English and Hinglish stopwords, a light plural stem, and aliases that map
//! Hinglish and Devanagari words to their meaning ("padho" → read). The word lists are data
//! (`data/words.json`), not code.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Aliases, stopwords and action words (data, not code)
pub struct Words {
    pub aliases: HashMap<String, Vec<String>>,
    pub stopwords: HashSet<String>,
    pub actions: HashSet<String>,
    /// Words that point back at the previous message ("isko", "that")
    pub followup: HashSet<String>,
    /// Words that ask for the previous request again ("phir se", "again")
    pub repeat: HashSet<String>,
}

pub static WORDS: LazyLock<Words> = LazyLock::new(|| {
    let data: Value = serde_json::from_str(include_str!("../data/words.json")).unwrap_or_default();
    let list = |key: &str| -> HashSet<String> {
        data.get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let aliases = data
        .get("aliases")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(alias, terms)| {
                    let terms = terms.as_array().map(|t| t.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                    (alias.to_lowercase(), terms)
                })
                .collect()
        })
        .unwrap_or_default();
    Words { aliases, stopwords: list("stopwords"), actions: list("actions"), followup: list("followup"), repeat: list("repeat") }
});

fn is_word_char(c: char) -> bool {
    // Devanagari vowel signs are marks, not letters, but they belong to the word
    c.is_alphanumeric() || ('\u{0900}'..='\u{097F}').contains(&c)
}

/// Raw lowercase words: split on anything that is not part of a word, and on camelCase,
/// snake_case and kebab-case boundaries
pub fn raw_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for c in text.chars() {
        if is_word_char(c) {
            if c.is_uppercase() && prev_lower && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.extend(c.to_lowercase());
            prev_lower = c.is_lowercase();
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            prev_lower = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Plural `s` is dropped so "files" and "file" meet ("vulnerabilities" and "vulnerability" too)
pub fn stem(word: &str) -> String {
    if word.len() > 4 && word.ends_with("ies") && word.is_ascii() {
        format!("{}y", &word[..word.len() - 3])
    } else if word.len() > 3 && word.ends_with('s') && !word.ends_with("ss") && word.is_ascii() {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

/// Search terms of a text: words without stopwords, with aliases mapped to their meaning
pub fn terms(text: &str) -> Vec<String> {
    let words = &*WORDS;
    let mut out = Vec::new();
    for word in raw_words(text) {
        // Numbers say nothing about which tool is meant
        if words.stopwords.contains(&word) || word.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        match words.aliases.get(&word) {
            Some(meanings) => out.extend(meanings.iter().cloned()),
            None => out.push(stem(&word)),
        }
    }
    out
}
