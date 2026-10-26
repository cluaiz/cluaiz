//! Text analysis shared by the tool matcher and the chat memory: whole words (split on case,
//! `_` and `-` too) and a light plural stem. No word lists of any language: how common a word is
//! comes from the text being searched (IDF), so "hai", "the" or "என்ன" weigh little on their own.

/// Letters, digits and the vowel signs of any script (Devanagari, Tamil, Bengali… write them as
/// marks, which are not alphanumeric); spaces and punctuation of any script split words
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || (!c.is_ascii() && !c.is_whitespace() && !is_punctuation(c))
}

/// Punctuation and symbols outside ASCII: Latin-1 (¿ ¡ « » ·), general punctuation (“ ” — …),
/// dandas (। ॥), CJK and full-width punctuation
fn is_punctuation(c: char) -> bool {
    matches!(
        c,
        '\u{00A0}'..='\u{00BF}'
            | '\u{00D7}'
            | '\u{00F7}'
            | '\u{0964}'..='\u{0965}'
            | '\u{2000}'..='\u{206F}'
            | '\u{3000}'..='\u{303F}'
            | '\u{FF01}'..='\u{FF0F}'
            | '\u{FF1A}'..='\u{FF20}'
            | '\u{FF3B}'..='\u{FF40}'
            | '\u{FF5B}'..='\u{FF65}'
    )
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

/// Search terms of a text: its words, plural-stemmed. Bare numbers are left out (they say
/// nothing about which tool is meant; values are matched as entities).
pub fn terms(text: &str) -> Vec<String> {
    raw_words(text).into_iter().filter(|word| !word.chars().all(|c| c.is_ascii_digit())).map(|word| stem(&word)).collect()
}
