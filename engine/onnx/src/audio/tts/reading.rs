//! Shared spoken-text preparation. Language words and break policy live in JSON.

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Deserialize)]
struct Rules {
    chunk_bytes: usize,
    silence_fraction: f32,
    max_pause_factor: usize,
    boundary_guard_ms: usize,
    paragraph_pause_ms: u32,
    numeric_infix: String,
    word_infix: String,
    /// Symbols that, standing alone between spaces (or at a line start), are a dash or a list
    /// bullet: a pause, not an operator word, unless numbers stand on both sides ("5 - 3")
    #[serde(default)]
    spaced_dashes: String,
    aliases: HashMap<char, char>,
    punctuation: HashMap<char, Boundary>,
    abbreviations: HashMap<String, Vec<String>>,
    operator_words: HashMap<String, HashMap<char, String>>,
}

#[derive(Deserialize)]
struct Boundary {
    emit: String,
    pause_ms: u32,
    #[serde(default)]
    opening: bool,
}

fn rules() -> Result<&'static Rules> {
    static RULES: OnceLock<Result<Rules, String>> = OnceLock::new();
    RULES
        .get_or_init(|| {
            let data: Rules = serde_json::from_str(include_str!("data/reading_rules.json"))
                .map_err(|e| e.to_string())?;
            if data.chunk_bytes == 0
                || data.max_pause_factor == 0
                || !(0.0..1.0).contains(&data.silence_fraction)
            {
                return Err("Invalid TTS reading limits".to_owned());
            }
            Ok(data)
        })
        .as_ref()
        .map_err(|e| anyhow!("TTS reading rules: {e}"))
}

pub(super) struct Chunk {
    pub text: String,
    pub pause_after_ms: u32,
}

/// Normalize symbols and retain every parenthetical word before phonemization.
pub(super) fn prepare(text: &str, language: &str) -> Result<Vec<Chunk>> {
    let rules = rules()?;
    let language = language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .to_lowercase();
    let operators = rules.operator_words.get(&language);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut line_breaks = 0;
    let all: Vec<char> = text.chars().collect();
    let mut at = 0usize; // index in `all` of the character being read
    let nearest = |from: usize, back: bool| -> Option<char> {
        let mut i = from;
        loop {
            i = if back { i.checked_sub(1)? } else { i + 1 };
            match all.get(i) {
                Some(c) if c.is_whitespace() => continue,
                other => return other.copied(),
            }
        }
    };
    for token in text.split_inclusive(char::is_whitespace) {
        let word = token.trim_end();
        // Keep URI/email structure; spoken URL spelling is a separate policy.
        let protected = word.contains("://")
            || word.contains('@')
            || rules
                .abbreviations
                .get(&language)
                .is_some_and(|words| words.contains(&word.to_lowercase()));
        let chars: Vec<char> = token.chars().collect();
        for (i, original) in chars.iter().copied().enumerate() {
            let here = at;
            at += 1;
            let c = rules.aliases.get(&original).copied().unwrap_or(original);
            let before = i.checked_sub(1).and_then(|i| chars.get(i)).copied();
            let after = chars.get(i + 1).copied();
            if !c.is_whitespace() {
                line_breaks = 0;
            }
            let numeric_infix = rules.numeric_infix.contains(c)
                && before.is_some_and(char::is_numeric)
                && after.is_some_and(char::is_numeric);
            let word_infix = rules.word_infix.contains(c)
                && before.is_some_and(char::is_alphabetic)
                && after.is_some_and(char::is_alphabetic);
            let internal_dot = c == '.'
                && before.is_some_and(char::is_alphabetic)
                && after.is_some_and(char::is_alphabetic);
            let spaced_dash = rules.spaced_dashes.contains(c)
                && before.map_or(true, char::is_whitespace)
                && after.map_or(true, char::is_whitespace)
                && !(nearest(here, true).is_some_and(char::is_numeric)
                    && nearest(here, false).is_some_and(char::is_numeric));
            if spaced_dash {
                if let Some(dash) = rules.punctuation.get(&'—') {
                    current.push(' ');
                    current.push_str(&dash.emit);
                    emit(&mut current, &mut chunks, dash.pause_ms, rules.chunk_bytes);
                }
                continue;
            }
            if (protected && !c.is_whitespace()) || numeric_infix || word_infix || internal_dot {
                current.push(original);
            } else if let Some(boundary) = rules.punctuation.get(&c) {
                if c == '\n' {
                    line_breaks += 1;
                }
                let pause = if line_breaks > 1 {
                    rules.paragraph_pause_ms
                } else {
                    boundary.pause_ms
                };
                if boundary.opening {
                    emit(&mut current, &mut chunks, pause, rules.chunk_bytes);
                    current.push_str(&boundary.emit);
                    current.push(' ');
                } else {
                    current.push(' ');
                    current.push_str(&boundary.emit);
                    emit(&mut current, &mut chunks, pause, rules.chunk_bytes);
                }
            } else if rules
                .operator_words
                .values()
                .any(|words| words.contains_key(&c))
            {
                let spoken = operators.and_then(|words| words.get(&c)).ok_or_else(|| {
                    anyhow!("No TTS spoken form for symbol {c:?} in language {language}")
                })?;
                current.push(' ');
                current.push_str(spoken);
                current.push(' ');
            } else if c.is_whitespace() {
                current.push(' ');
            } else {
                current.push(c);
            }
        }
    }
    emit(&mut current, &mut chunks, 0, rules.chunk_bytes);
    if chunks.is_empty() {
        bail!("TTS input contains no readable text");
    }
    Ok(chunks)
}

fn emit(current: &mut String, chunks: &mut Vec<Chunk>, pause: u32, limit: usize) {
    let text = current.split_whitespace().collect::<Vec<_>>().join(" ");
    current.clear();
    if text.chars().any(char::is_alphanumeric) {
        chunks.extend(chunk_tts_text(&text, limit).into_iter().map(|text| Chunk {
            text,
            pause_after_ms: 0,
        }));
    } else if let Some(last) = chunks.last_mut() {
        // Repeated punctuation/closing brackets belong to the preceding phrase.
        last.text.push_str(&text);
    }
    if let Some(last) = chunks.last_mut() {
        last.pause_after_ms = last.pause_after_ms.max(pause);
    }
}

/// Word-preserving byte target; a single oversized word retains the native token guard.
pub(super) fn chunk_tts_text(text: &str, limit: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for word in text.split_inclusive(char::is_whitespace) {
        if current.len() + word.len() > limit && !current.trim().is_empty() {
            chunks.push(current.trim().to_owned());
            current.clear();
        }
        current.push_str(word);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_owned());
    }
    chunks
}

/// Bound quiet boundary padding relative to chunk gain; retain guards next to speech.
pub(super) fn append_audio(
    all: &mut Vec<f32>,
    pcm: &[f32],
    rate: usize,
    pause_ms: u32,
) -> Result<()> {
    if pcm.is_empty() {
        return Ok(());
    }
    if all.is_empty() {
        all.extend_from_slice(pcm);
    } else if pause_ms > 0 {
        let rules = rules()?;
        let peak = pcm.iter().copied().map(f32::abs).fold(0.0, f32::max);
        let threshold = rules.silence_fraction * peak;
        let quiet = |v: &&f32| v.abs() <= threshold;
        let trailing = all.iter().rev().take_while(quiet).count();
        let leading = pcm.iter().take_while(quiet).count();
        let mut existing = trailing + leading;
        let target = rate
            .checked_mul(pause_ms as usize)
            .context("TTS pause size overflow")?
            / 1000;
        let maximum = target
            .checked_mul(rules.max_pause_factor)
            .context("TTS pause limit overflow")?;
        let guard = rate
            .checked_mul(rules.boundary_guard_ms)
            .context("TTS guard size overflow")?
            / 1000;
        let excess = existing.saturating_sub(maximum);
        let drop_tail = excess.min(trailing.saturating_sub(guard));
        let drop_head = (excess - drop_tail).min(leading.saturating_sub(guard));
        all.truncate(all.len() - drop_tail);
        let pcm = &pcm[drop_head..];
        existing -= drop_tail + drop_head;
        let length = all
            .len()
            .checked_add(target.saturating_sub(existing))
            .context("TTS audio size overflow")?;
        all.resize(length, 0.0);
        all.extend_from_slice(pcm);
    } else {
        let overlap = 120.min(all.len()).min(pcm.len());
        let start = all.len() - overlap;
        for i in 0..overlap {
            let alpha = i as f32 / overlap as f32;
            all[start + i] = all[start + i] * (1.0 - alpha) + pcm[i] * alpha;
        }
        all.extend_from_slice(&pcm[overlap..]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Case {
        language: String,
        input: String,
        #[serde(default)]
        contains: Vec<String>,
        #[serde(default)]
        not_contains: Vec<String>,
        pause_ms: Option<Vec<u32>>,
        chunk_count: Option<usize>,
        #[serde(default)]
        error: bool,
    }

    #[test]
    fn test_reading_preserves_content_and_expands_symbols_at_semantic_boundaries() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("data/reading_tests.json")).unwrap();
        let cases: Vec<Case> = serde_json::from_value(fixture["cases"].clone()).unwrap();
        for case in cases {
            let result = prepare(&case.input, &case.language);
            if case.error {
                assert!(result.is_err(), "{}", case.input);
                continue;
            }
            let chunks = result.unwrap();
            let text = chunks
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            for expected in case.contains {
                assert!(text.contains(&expected), "{text:?} misses {expected:?}");
            }
            for unwanted in case.not_contains {
                assert!(!text.contains(&unwanted), "{text:?} contains {unwanted:?}");
            }
            if let Some(count) = case.chunk_count {
                assert_eq!(chunks.len(), count, "{text}");
            }
            if let Some(pauses) = case.pause_ms {
                assert_eq!(
                    chunks.iter().map(|c| c.pause_after_ms).collect::<Vec<_>>(),
                    pauses,
                    "{text}"
                );
            }
        }
    }

    #[test]
    fn test_operator_labels_exist_for_every_configured_language() {
        let rules = rules().unwrap();
        for (language, words) in &rules.operator_words {
            for (symbol, label) in words {
                // Between numbers, where every operator is spoken (a lone dash is a pause)
                let chunks = prepare(&format!("1{symbol}2"), language).unwrap();
                assert_eq!(chunks[0].text, format!("1 {label} 2"));
            }
        }
    }

    #[test]
    fn test_pause_join_adds_only_missing_quiet_samples_and_preserves_speech() {
        let mut all = vec![0.5];
        all.extend(vec![0.0; 10]);
        let mut next = vec![0.0; 20];
        next.push(0.5);
        append_audio(&mut all, &next, 1000, 120).unwrap();
        assert_eq!(all.len(), 122);
        assert_eq!(all.iter().filter(|v| **v == 0.0).count(), 120);
        let length = all.len();
        let mut native_gap = vec![0.0; 200];
        native_gap.push(0.5);
        append_audio(&mut all, &native_gap, 1000, 120).unwrap();
        assert_eq!(all.len(), length + native_gap.len());
        assert_eq!(all.first(), Some(&0.5));
        assert_eq!(all.last(), Some(&0.5));
    }

    #[test]
    fn test_pause_join_caps_only_near_zero_padding_and_keeps_boundary_guards() {
        let mut all = vec![0.5];
        all.extend(vec![0.0; 400]);
        let mut next = vec![0.0; 400];
        next.push(0.25);
        append_audio(&mut all, &next, 1000, 120).unwrap();
        assert_eq!(all.len(), 242);
        assert_eq!(all.first(), Some(&0.5));
        assert_eq!(all.last(), Some(&0.25));
        // Soft non-silent phonemes are not eligible for padding removal.
        let soft = rules().unwrap().silence_fraction * 2.0;
        let length = all.len();
        append_audio(&mut all, &vec![soft; 400], 1000, 120).unwrap();
        assert_eq!(&all[all.len() - 400..], &vec![soft; 400]);
        assert!(all.len() >= length + 400);
    }

    #[test]
    fn test_pause_join_detects_low_padding_at_different_signal_gains() {
        for gain in [0.1, 0.8] {
            let quiet = rules().unwrap().silence_fraction * gain * 0.5;
            let mut all = vec![gain];
            all.extend(vec![quiet; 400]);
            let mut next = vec![quiet; 400];
            next.push(gain);
            append_audio(&mut all, &next, 1000, 120).unwrap();
            assert_eq!(all.len(), 242);
            assert_eq!(all.first(), Some(&gain));
            assert_eq!(all.last(), Some(&gain));
            assert_eq!(all.iter().filter(|v| **v == quiet).count(), 240);
        }
    }
}
