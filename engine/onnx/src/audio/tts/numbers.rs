//! Numbers said as words before G2P ("₹300" → "तीन सौ रुपये", "2026" → "two thousand twenty-six").
//! One JSON file per language in `data/numbers/` (embedded at build time); a language without a
//! file uses English. Runs on prepared text, where operators between numbers are already words.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Numbers {
    language: String,
    /// Words for these numbers (Hindi lists 0–99, English 0–19 and the tens)
    small: HashMap<u64, String>,
    /// Between a tens word and a unit word ("twenty-one")
    tens_join: String,
    /// (value, word), largest first
    scales: Vec<(u64, String)>,
    point: String,
    /// Digit runs at least this long (phone numbers) are read digit by digit
    read_digits_from: usize,
    /// Symbol before a number → word said after it
    #[serde(default)]
    currency: HashMap<char, String>,
    /// Symbol after a number → word said after it
    #[serde(default)]
    suffixes: HashMap<char, String>,
    /// Operators said only between two numbers ("2*2")
    #[serde(default)]
    between_numbers: HashMap<char, String>,
    #[serde(default)]
    #[cfg_attr(not(test), allow(dead_code))]
    examples: HashMap<String, String>,
}

fn all() -> &'static [Numbers] {
    static ALL: OnceLock<Vec<Numbers>> = OnceLock::new();
    ALL.get_or_init(|| {
        super::rules_g2p::NUMBER_FILES
            .iter()
            .filter_map(|(name, text)| match serde_json::from_str(text) {
                Ok(n) => Some(n),
                Err(e) => {
                    tracing::error!("🔢 [Numbers] {name} is invalid: {e}");
                    None
                }
            })
            .collect()
    })
}

fn for_language(language: &str) -> Option<&'static Numbers> {
    let base = language.split(['-', '_']).next().unwrap_or(language).to_lowercase();
    all()
        .iter()
        .find(|n| n.language == base)
        .or_else(|| all().iter().find(|n| n.language == "en"))
}

/// Text with every number written as words of `language`.
pub(super) fn expand(text: &str, language: &str) -> String {
    let Some(numbers) = for_language(language) else {
        return text.to_owned();
    };
    if !text.chars().any(|c| c.is_ascii_digit()) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() * 3);
    let mut i = 0;
    while i < chars.len() {
        let currency = numbers
            .currency
            .get(&chars[i])
            .filter(|_| chars.get(i + 1).is_some_and(char::is_ascii_digit));
        let start = if currency.is_some() { i + 1 } else { i };
        if !chars[start].is_ascii_digit() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let (spoken, end) = numbers.read_number(&chars, start);
        push_word(&mut out, &spoken);
        // "₹300 रुपये": the currency word is said once
        let next: String = chars[end..].iter().skip_while(|c| c.is_whitespace()).collect();
        if let Some(word) = currency.filter(|w| !next.starts_with(w.as_str())) {
            push_word(&mut out, word);
        }
        i = end;
        if let Some(word) = chars.get(i).and_then(|c| numbers.suffixes.get(c)) {
            push_word(&mut out, word);
            i += 1;
        }
        // "2*2": an operator only numbers stand around
        if let Some(word) = chars.get(i).and_then(|c| numbers.between_numbers.get(c)) {
            if chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                push_word(&mut out, word);
                i += 1;
            }
        }
    }
    // Words were added with spaces around them; keep one where the text already had one
    let mut tidy = String::with_capacity(out.len());
    for c in out.chars() {
        if !(c == ' ' && tidy.ends_with(' ')) {
            tidy.push(c);
        }
    }
    tidy
}

/// Appends a word with a space before it when the text does not already end with one.
fn push_word(out: &mut String, word: &str) {
    if !out.is_empty() && !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
    out.push_str(word);
    out.push(' ');
}

impl Numbers {
    /// Reads the number starting at `start` (digits with group commas, a decimal part, or a time
    /// "12:30"); returns its words and the index after it.
    fn read_number(&self, chars: &[char], start: usize) -> (String, usize) {
        let mut i = start;
        let mut digits = String::new();
        // Group separators: a comma followed by digits ("1,000", "1,50,000")
        while i < chars.len() {
            if chars[i].is_ascii_digit() {
                digits.push(chars[i]);
                i += 1;
            } else if chars[i] == ',' && chars.get(i + 1).is_some_and(char::is_ascii_digit) && !digits.is_empty()
            {
                i += 1;
            } else {
                break;
            }
        }
        let mut spoken = if digits.len() >= self.read_digits_from || (digits.len() > 1 && digits.starts_with('0')) {
            self.digit_by_digit(&digits)
        } else {
            digits.parse::<u64>().map_or_else(|_| self.digit_by_digit(&digits), |n| self.words(n))
        };
        // Decimal part, read digit by digit; a time's minutes as a number
        match (chars.get(i), chars.get(i + 1)) {
            (Some('.'), Some(d)) if d.is_ascii_digit() => {
                let decimals: String = chars[i + 1..].iter().take_while(|c| c.is_ascii_digit()).collect();
                spoken = format!("{spoken} {} {}", self.point, self.digit_by_digit(&decimals));
                i += 1 + decimals.len();
            }
            (Some(':'), Some(d)) if d.is_ascii_digit() => {
                let minutes: String = chars[i + 1..].iter().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(m) = minutes.parse::<u64>() {
                    spoken = format!("{spoken} {}", self.words(m));
                }
                i += 1 + minutes.len();
            }
            _ => {}
        }
        (spoken, i)
    }

    fn digit_by_digit(&self, digits: &str) -> String {
        digits
            .chars()
            .filter_map(|d| d.to_digit(10).and_then(|d| self.small.get(&u64::from(d))).cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn words(&self, n: u64) -> String {
        if let Some(word) = self.small.get(&n) {
            return word.clone();
        }
        if let Some(&(scale, ref name)) = self.scales.iter().find(|(scale, _)| n >= *scale) {
            let (quotient, rest) = (n / scale, n % scale);
            let head = format!("{} {name}", self.words(quotient));
            return if rest == 0 { head } else { format!("{head} {}", self.words(rest)) };
        }
        // Below the smallest scale and not listed: tens + unit
        let (tens, unit) = (n / 10 * 10, n % 10);
        match (self.small.get(&tens), self.small.get(&unit)) {
            (Some(t), Some(u)) => format!("{t}{}{u}", self.tens_join),
            _ => self.digit_by_digit(&n.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_number_file_says_its_examples() {
        let mut checked = 0;
        for numbers in all() {
            for (input, expected) in &numbers.examples {
                assert_eq!(expand(input, &numbers.language).trim(), expected, "{} {input}", numbers.language);
                checked += 1;
            }
        }
        assert!(checked >= 10, "only {checked} examples");
    }

    #[test]
    fn numbers_inside_text_keep_the_words_around_them() {
        let words = |s: String| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(words(expand("ये ₹300 रुपये लो", "hi")), "ये तीन सौ रुपये लो");
        assert_eq!(words(expand("ये ₹300 लो", "hi")), "ये तीन सौ रुपये लो");
        assert_eq!(expand("No numbers here.", "en"), "No numbers here.");
        assert_eq!(expand("minus 2 and 7 minus 3", "en").split_whitespace().collect::<Vec<_>>().join(" "),
                   "minus two and seven minus three");
    }
}
