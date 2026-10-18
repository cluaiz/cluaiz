//! Text language from its script, and voice language from its file name (`data/languages.json`).

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

const LANGUAGES_JSON: &str = include_str!("data/languages.json");

#[derive(Deserialize)]
struct Languages {
    scripts: Vec<Script>,
    #[serde(default)]
    preferred_voices: Vec<String>,
    voice_prefix_languages: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Script {
    language: String,
    ranges: Vec<(String, String)>,
}

fn languages() -> Option<&'static Languages> {
    static LANGUAGES: OnceLock<Option<Languages>> = OnceLock::new();
    LANGUAGES
        .get_or_init(|| match serde_json::from_str(LANGUAGES_JSON) {
            Ok(languages) => Some(languages),
            Err(e) => {
                tracing::error!("🔤 [Languages] languages.json is invalid: {e}");
                None
            }
        })
        .as_ref()
}

fn base(lang: &str) -> &str {
    lang.split(['-', '_']).next().unwrap_or(lang)
}

/// The language a text's script names (the script most of its letters use), if it is listed.
pub fn script_language(text: &str) -> Option<&'static str> {
    let languages = languages()?;
    let in_range = |c: char, (lo, hi): &(String, String)| {
        let (Ok(lo), Ok(hi)) = (u32::from_str_radix(lo, 16), u32::from_str_radix(hi, 16)) else {
            return false;
        };
        (lo..=hi).contains(&(c as u32))
    };
    languages
        .scripts
        .iter()
        .map(|script| {
            let count = text
                .chars()
                .filter(|&c| script.ranges.iter().any(|r| in_range(c, r)))
                .count();
            (script, count)
        })
        .filter(|(_, count)| *count > 0)
        .max_by_key(|(_, count)| *count)
        .map(|(script, _)| script.language.as_str())
}

/// A voice file of a model (`voices/<id>.bin`) and the language its name says it speaks.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Voice {
    pub id: String,
    pub language: Option<String>,
}

/// The model's voice files, preferred voices first (in their listed order), then by name. Empty for
/// a single-voice model.
pub fn list_voices(model_dir: &Path) -> Vec<Voice> {
    let Some(languages) = languages() else {
        return Vec::new();
    };
    let mut ids: Vec<String> = std::fs::read_dir(model_dir.join("voices"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "bin"))
        .filter_map(|p| p.file_stem()?.to_str().map(str::to_owned))
        .collect();
    let rank = |v: &String| {
        languages
            .preferred_voices
            .iter()
            .position(|p| p == v)
            .unwrap_or(usize::MAX)
    };
    ids.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
    ids.into_iter()
        .map(|id| {
            let language = id
                .chars()
                .next()
                .and_then(|prefix| languages.voice_prefix_languages.get(&prefix.to_string()))
                .cloned();
            Voice { id, language }
        })
        .collect()
}

/// The voice file (stem in `voices/`) to use for `lang`: an exact language match first, then the
/// same base language; preferred voices first, then by name. None when no voice's name says it speaks that language.
pub fn voice_for_language(model_dir: &Path, lang: &str) -> Option<String> {
    let voices = list_voices(model_dir);
    let lang = lang.to_lowercase();
    voices
        .iter()
        .find(|v| v.language.as_deref() == Some(lang.as_str()))
        .or_else(|| voices.iter().find(|v| v.language.as_deref().map(base) == Some(base(&lang))))
        .map(|v| v.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_picks_the_language_most_letters_use() {
        assert_eq!(script_language("नमस्ते दुनिया, hello"), Some("hi"));
        assert_eq!(script_language("hello world"), None);
    }
}
