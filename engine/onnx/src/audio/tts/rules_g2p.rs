//! Letter-to-sound rules (grapheme → phoneme) for languages written close to how they are spoken,
//! one JSON file per language in `data/g2p_rules/` (embedded at build time). Files placed in
//! `<cluaiz home>/data/g2p_rules/` are read too and replace an embedded file of the same language,
//! so a language can be added or corrected without a new engine build. Used for words no
//! dictionary has.
//!
//! Two kinds of writing systems:
//! - `abugida` (Indian scripts): a consonant carries an inherent vowel unless a vowel sign or virama
//!   follows; languages that do not say it drop it at the end of a word and between
//!   a vowel-consonant and a consonant-vowel, right to left.
//! - `alphabet` (Latin, Greek, Cyrillic...): ordered rewrite rules, longest spelling first, each
//!   optionally limited to letters before/after it ("c" before e/i → tʃ in Italian).
//!
//! A script shared by several languages (Devanagari: Hindi, Marathi, Nepali) uses the file of the
//! model's language, else the file marked `default_for_script`. Alphabet rules are used only for a
//! model speaking that language, so Latin words never get another language's rules.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/g2p_rule_files.rs"));

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum RuleSet {
    Abugida(Abugida),
    Alphabet(Alphabet),
}

#[derive(Deserialize)]
struct Abugida {
    language: String,
    script_ranges: Vec<(String, String)>,
    /// Opt in to NFC only when this rule file uses canonical spellings for its keys.
    #[serde(default)]
    normalize_unicode: bool,
    /// Formatting controls/elision signs that do not change pronunciation.
    #[serde(default)]
    ignored_characters: String,
    /// Internal word boundaries; schwa rules apply to each piece separately.
    #[serde(default)]
    word_separators: String,
    #[serde(default)]
    default_for_script: bool,
    consonants: HashMap<String, String>,
    /// Consonant letters that never carry the inherent vowel.
    #[serde(default)]
    dead_consonants: HashMap<String, String>,
    vowels: HashMap<String, String>,
    vowel_signs: HashMap<String, String>,
    inherent_vowel: String,
    virama: String,
    /// Vowel said for a virama at the end of a word (Malayalam ŭ); none when absent
    #[serde(default)]
    final_virama_vowel: Option<String>,
    #[serde(default)]
    nasalize_marks: Vec<String>,
    #[serde(default)]
    anusvara_marks: Vec<String>,
    /// Said for an anusvara not followed by a consonant of a nasal class; the vowel is nasalized
    /// when absent
    #[serde(default)]
    anusvara_otherwise: Option<String>,
    /// Doubles the next consonant (Gurmukhi addak)
    #[serde(default)]
    gemination_mark: Option<String>,
    #[serde(default)]
    visarga: Option<Visarga>,
    nasal_vowel_mark: String,
    #[serde(default)]
    anusvara_before: HashMap<String, Vec<String>>,
    /// Consonants voiced between vowels and after nasals.
    #[serde(default)]
    voicing: Option<Voicing>,
    #[serde(default)]
    punctuation: HashMap<String, String>,
    schwa_deletion: SchwaDeletion,
    /// Word → expected IPA, checked by the tests
    #[serde(default)]
    #[cfg_attr(not(test), allow(dead_code))]
    examples: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Visarga {
    mark: String,
    ipa: String,
}

#[derive(Deserialize)]
struct Voicing {
    map: HashMap<String, String>,
    nasals: Vec<String>,
}

#[derive(Deserialize)]
struct SchwaDeletion {
    word_final: bool,
    keep_final_after_cluster: bool,
    medial_vc_cv: bool,
}

#[derive(Deserialize)]
struct Alphabet {
    language: String,
    script_ranges: Vec<(String, String)>,
    /// Named letter sets the rules refer to ("V": vowels)
    #[serde(default)]
    classes: HashMap<String, String>,
    rules: Vec<Rewrite>,
    /// Word → expected IPA, checked by the tests
    #[serde(default)]
    #[cfg_attr(not(test), allow(dead_code))]
    examples: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Rewrite {
    /// Spelling matched
    g: String,
    /// Phonemes said
    p: String,
    /// The next letter must be in this class (name or letters); "#" = end of word
    #[serde(default)]
    before: Option<String>,
    /// The previous letter must be in this class (name or letters); "#" = start of word
    #[serde(default)]
    after: Option<String>,
}

fn rule_sets() -> &'static [RuleSet] {
    static RULES: OnceLock<Vec<RuleSet>> = OnceLock::new();
    RULES.get_or_init(|| {
        let mut by_language: Vec<(String, RuleSet)> = Vec::new();
        let mut add = |name: &str, text: &str| match serde_json::from_str::<RuleSet>(text) {
            Ok(set) => {
                let lang = set.language().to_owned();
                by_language.retain(|(l, _)| *l != lang);
                by_language.push((lang, set));
            }
            Err(e) => tracing::error!("🔤 [Rules G2P] {name} is invalid: {e}"),
        };
        for (name, text) in RULE_FILES {
            add(name, text);
        }
        let user_dir = cluaiz_core::EnvironmentManager::current()
            .global_dir
            .join("data")
            .join("g2p_rules");
        for path in std::fs::read_dir(user_dir).into_iter().flatten().flatten().map(|e| e.path()) {
            if path.extension().is_some_and(|x| x == "json") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    add(&path.display().to_string(), &text);
                }
            }
        }
        by_language.into_iter().map(|(_, set)| set).collect()
    })
}

fn in_ranges(c: char, ranges: &[(String, String)]) -> bool {
    ranges.iter().any(|(lo, hi)| {
        match (u32::from_str_radix(lo, 16), u32::from_str_radix(hi, 16)) {
            (Ok(lo), Ok(hi)) => (lo..=hi).contains(&(c as u32)),
            _ => false,
        }
    })
}

fn base(lang: &str) -> &str {
    lang.split(['-', '_']).next().unwrap_or(lang)
}

/// IPA and language for a word, by the rules for its script and the model's language `lang`.
/// None when no rules apply (the caller tries dictionary pieces, then the spelling).
pub fn word_to_ipa(word: &str, lang: &str) -> Option<(String, &'static str)> {
    if word.is_empty() {
        return None;
    }
    let lang = base(lang).to_lowercase();
    let covers = |set: &RuleSet| {
        let in_script = |c| in_ranges(c, set.script_ranges());
        word.chars().any(in_script) && word.chars().all(|c| {
            in_script(c) || matches!(set, RuleSet::Abugida(a)
                if a.ignored_characters.contains(c) || a.word_separators.contains(c))
        })
    };
    let sets = rule_sets();
    let preferred = sets
        .iter()
        .find(|s| s.language() == lang && word.chars().any(|c| in_ranges(c, s.script_ranges())));
    let set = preferred
        .or_else(|| {
            sets.iter().find(|s| {
                matches!(s, RuleSet::Abugida(a) if a.default_for_script) && covers(s)
            })
        })?;
    if !covers(set) {
        return None;
    }
    Some((set.convert(word), set.language()))
}

impl RuleSet {
    fn language(&self) -> &str {
        match self {
            RuleSet::Abugida(a) => &a.language,
            RuleSet::Alphabet(a) => &a.language,
        }
    }

    #[cfg(test)]
    fn examples(&self) -> &HashMap<String, String> {
        match self {
            RuleSet::Abugida(a) => &a.examples,
            RuleSet::Alphabet(a) => &a.examples,
        }
    }

    fn script_ranges(&self) -> &[(String, String)] {
        match self {
            RuleSet::Abugida(a) => &a.script_ranges,
            RuleSet::Alphabet(a) => &a.script_ranges,
        }
    }

    fn convert(&self, word: &str) -> String {
        match self {
            RuleSet::Abugida(a) => a.convert(word),
            RuleSet::Alphabet(a) => a.convert(word),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Consonant,
    Vowel,
}

struct Segment {
    ipa: String,
    kind: Kind,
    /// The consonant's own vowel (no sign written): the only vowel that can be dropped
    inherent: bool,
    anusvara: bool,
    nasal: bool,
}

impl Segment {
    fn new(ipa: &str, kind: Kind, inherent: bool) -> Self {
        Self {
            ipa: ipa.to_owned(),
            kind,
            inherent,
            anusvara: false,
            nasal: false,
        }
    }
}

/// Longest key of `map` at the start of `rest` (nukta letters and split vowel signs are two or
/// three code points).
fn longest<'m>(map: &'m HashMap<String, String>, rest: &str) -> Option<(usize, &'m str)> {
    let ends: Vec<usize> = rest
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .take(3)
        .collect();
    ends.into_iter()
        .rev()
        .find_map(|end| map.get(&rest[..end]).map(|ipa| (end, ipa.as_str())))
}

impl Abugida {
    fn convert(&self, word: &str) -> String {
        // Canonically equivalent spellings must use the same data-owned rules.
        let mut normalized = tokenizers::NormalizedString::from(word);
        if self.normalize_unicode {
            normalized.nfc();
        }
        let word = normalized.get();
        if !word.chars().any(|c| self.ignored_characters.contains(c) || self.word_separators.contains(c)) {
            return self.convert_word(word);
        }
        let clean: String = word.chars().filter(|&c| !self.ignored_characters.contains(c)).collect();
        clean.split(|c| self.word_separators.contains(c))
            .filter(|part| !part.is_empty())
            .map(|part| self.convert_word(part))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn convert_word(&self, word: &str) -> String {
        let mut segs: Vec<Segment> = Vec::new();
        let mut tail = String::new();
        let mut geminate = false;
        let mut i = 0;
        while i < word.len() {
            let rest = &word[i..];
            if let Some((len, ipa)) = longest(&self.dead_consonants, rest) {
                segs.push(Segment::new(ipa, Kind::Consonant, false));
                i += len;
            } else if let Some((len, ipa)) = longest(&self.consonants, rest) {
                if std::mem::take(&mut geminate) {
                    segs.push(Segment::new(ipa, Kind::Consonant, false));
                }
                segs.push(Segment::new(ipa, Kind::Consonant, false));
                i += len;
                let next = &word[i..];
                if let Some((len, sign)) = longest(&self.vowel_signs, next) {
                    segs.push(Segment::new(sign, Kind::Vowel, false));
                    i += len;
                } else if next.starts_with(&self.virama) {
                    i += self.virama.len();
                    if i >= word.len() {
                        if let Some(v) = &self.final_virama_vowel {
                            segs.push(Segment::new(v, Kind::Vowel, false));
                        }
                    }
                } else {
                    segs.push(Segment::new(&self.inherent_vowel, Kind::Vowel, true));
                }
            } else if let Some((len, ipa)) = longest(&self.vowels, rest) {
                segs.push(Segment::new(ipa, Kind::Vowel, false));
                i += len;
            } else {
                let c = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                let mark = &rest[..c];
                let last_vowel = segs.iter_mut().rev().find(|s| s.kind == Kind::Vowel);
                if self.anusvara_marks.iter().any(|m| m == mark) {
                    if let Some(v) = last_vowel {
                        v.anusvara = true;
                    }
                } else if self.nasalize_marks.iter().any(|m| m == mark) {
                    if let Some(v) = last_vowel {
                        v.nasal = true;
                    }
                } else if self.gemination_mark.as_deref() == Some(mark) {
                    geminate = true;
                } else if let Some(visarga) = self.visarga.as_ref().filter(|v| v.mark == mark) {
                    segs.push(Segment::new(&visarga.ipa, Kind::Consonant, false));
                } else if let Some(p) = self.punctuation.get(mark) {
                    tail.push_str(p);
                }
                i += c;
            }
        }
        self.drop_schwas(&mut segs);
        self.voice(&mut segs);
        let mut out = String::new();
        for (k, seg) in segs.iter().enumerate() {
            if seg.kind != Kind::Vowel || !(seg.nasal || seg.anusvara) {
                out.push_str(&seg.ipa);
                continue;
            }
            let nasal_stop = seg
                .anusvara
                .then(|| segs.get(k + 1))
                .flatten()
                .filter(|next| next.kind == Kind::Consonant)
                .and_then(|next| {
                    self.anusvara_before
                        .iter()
                        .find(|(_, before)| before.iter().any(|b| *b == next.ipa))
                        .map(|(nasal, _)| nasal.as_str())
                });
            match (nasal_stop, seg.anusvara, self.anusvara_otherwise.as_deref()) {
                (Some(nasal), _, _) | (None, true, Some(nasal)) => {
                    out.push_str(&seg.ipa);
                    out.push_str(nasal);
                }
                _ => out.push_str(&self.nasalized(&seg.ipa)),
            }
        }
        out.push_str(&tail);
        out
    }

    /// The vowel with the nasal mark after its first letter ("aː" → "ãː").
    fn nasalized(&self, vowel: &str) -> String {
        let mut chars = vowel.chars();
        let first = chars.next().map(String::from).unwrap_or_default();
        format!("{first}{}{}", self.nasal_vowel_mark, chars.as_str())
    }

    fn drop_schwas(&self, segs: &mut Vec<Segment>) {
        let droppable = |s: &Segment| s.kind == Kind::Vowel && s.inherent && !s.anusvara && !s.nasal;
        let vowels = segs.iter().filter(|s| s.kind == Kind::Vowel).count();
        let rules = &self.schwa_deletion;
        if rules.word_final && vowels > 1 {
            if let Some(last) = segs.last() {
                let n = segs.len();
                let after_cluster = n >= 3
                    && segs[n - 2].kind == Kind::Consonant
                    && segs[n - 3].kind == Kind::Consonant;
                if droppable(last) && !(rules.keep_final_after_cluster && after_cluster) {
                    segs.pop();
                }
            }
        }
        if rules.medial_vc_cv {
            let mut i = segs.len().saturating_sub(3);
            while i >= 2 {
                let vc_cv = segs[i - 2].kind == Kind::Vowel
                    && segs[i - 1].kind == Kind::Consonant
                    && segs[i + 1].kind == Kind::Consonant
                    && segs.get(i + 2).is_some_and(|s| s.kind == Kind::Vowel);
                if droppable(&segs[i]) && vc_cv {
                    segs.remove(i);
                }
                i -= 1;
            }
        }
    }

    /// Voices a consonant between vowels or after a nasal (a doubled consonant stays voiceless).
    fn voice(&self, segs: &mut [Segment]) {
        let Some(voicing) = &self.voicing else {
            return;
        };
        for i in 1..segs.len() {
            let Some(voiced) = voicing.map.get(&segs[i].ipa) else {
                continue;
            };
            let prev = &segs[i - 1];
            let between_vowels = prev.kind == Kind::Vowel
                && segs.get(i + 1).is_some_and(|s| s.kind == Kind::Vowel);
            let after_nasal = prev.kind == Kind::Consonant && voicing.nasals.contains(&prev.ipa);
            if between_vowels || after_nasal {
                segs[i].ipa = voiced.clone();
            }
        }
    }
}

impl Alphabet {
    fn class_has(&self, class: &str, c: Option<char>) -> bool {
        if class == "#" {
            return c.is_none();
        }
        let Some(c) = c else {
            return false;
        };
        self.classes.get(class).map_or(class, String::as_str).contains(c)
    }

    fn convert(&self, word: &str) -> String {
        let word = word.to_lowercase();
        let chars: Vec<char> = word.chars().collect();
        // Longest spelling first; file order decides between rules of the same length
        let mut rules: Vec<&Rewrite> = self.rules.iter().collect();
        rules.sort_by_key(|r| std::cmp::Reverse(r.g.chars().count()));
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let found = rules.iter().find(|r| {
                let g: Vec<char> = r.g.chars().collect();
                let end = i + g.len();
                end <= chars.len()
                    && chars[i..end] == g[..]
                    && r.before.as_deref().map_or(true, |b| self.class_has(b, chars.get(end).copied()))
                    && r.after.as_deref().map_or(true, |a| {
                        self.class_has(a, i.checked_sub(1).map(|p| chars[p]))
                    })
            });
            match found {
                Some(rule) => {
                    out.push_str(&rule.p);
                    i += rule.g.chars().count();
                }
                None => i += 1,
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Case {
        group: String,
        text: String,
        language: String,
        expected_language: Option<String>,
        expected_ipa: Option<String>,
        same_as: Option<String>,
        #[serde(default)]
        reject: bool,
        #[serde(default)]
        model_coverage: bool,
    }

    fn cases() -> Vec<Case> {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("data/pronunciation_tests.json")).unwrap();
        serde_json::from_value(fixture["rules"].clone()).unwrap()
    }

    fn check_group(group: &str) {
        let mut checked = 0;
        for case in cases().into_iter().filter(|case| case.group == group) {
            let got = word_to_ipa(&case.text, &case.language);
            if case.reject {
                assert!(got.is_none(), "unexpected rule for {}", case.text);
            } else {
                let (ipa, language) = got.expect("fixture must select a rule");
                if let Some(expected) = case.expected_ipa {
                    assert_eq!(ipa, expected, "{}", case.text);
                }
                if let Some(expected) = case.expected_language {
                    assert_eq!(language, expected, "{}", case.text);
                }
                if let Some(equivalent) = case.same_as {
                    assert_eq!(Some((ipa, language)), word_to_ipa(&equivalent, &case.language), "{}", case.text);
                }
            }
            checked += 1;
        }
        assert!(checked > 0, "empty fixture group {group}");
    }

    /// Every rule file's `examples` (word → expected IPA) hold, so a rule edit that breaks a
    /// language fails here.
    #[test]
    fn every_rule_file_says_its_examples() {
        let mut checked = 0;
        for set in rule_sets() {
            for (word, expected) in set.examples() {
                let got = word_to_ipa(word, set.language()).map(|(ipa, _)| ipa);
                assert_eq!(got.as_deref(), Some(expected.as_str()), "{} {word}", set.language());
                checked += 1;
            }
        }
        assert!(checked >= 50, "only {checked} examples");
    }

    #[test]
    fn script_and_language_pick_the_rules() {
        check_group("routing");
    }

    #[test]
    fn separators_and_join_controls_preserve_words() {
        check_group("boundary");
    }

    #[test]
    fn canonical_spellings_keep_the_same_pronunciation() {
        check_group("canonical");
    }

    #[test]
    #[ignore = "requires CLUAIZ_TTS_TEST_MODEL pointing to staged genuine model metadata"]
    fn actual_pronunciations_fit_installed_model_symbols() {
        let dir = std::path::PathBuf::from(std::env::var("CLUAIZ_TTS_TEST_MODEL").unwrap());
        let map = super::super::phoneme_map::PhonemeMap::from_model_dir(&dir).unwrap();
        for case in cases().into_iter().filter(|case| case.model_coverage) {
            let converter = super::super::phoneme_sets::converter_for(&dir, &case.language);
            let (ipa, _) = word_to_ipa(&case.text, &case.language).unwrap();
            let converted = converter.as_ref().map_or(ipa.clone(), |c| c.convert(&ipa));
            let missing: String = converted.chars().filter(|c| !c.is_whitespace())
                .filter(|c| !map.has_symbol(&c.to_string())).collect();
            assert!(missing.is_empty(), "unsupported symbols: {missing}");
            let ids = map.text_to_ids_no_pad(&converted);
            map.require_phonemes(&ids).unwrap();
            println!("source_chars={} phoneme_chars={} model_tokens={} missing_symbols=0",
                case.text.chars().count(), converted.chars().count(), ids.len());
        }
    }
}
