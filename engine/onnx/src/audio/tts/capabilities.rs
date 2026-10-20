//! Discovery without allocating inference sessions. Voice assets and language controls are separate.
use super::{family_adapter::{FamilyAdapter, TtsFamily}, languages::list_voices};
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct TtsCapabilities {
    pub languages: Vec<String>,
    /// `explicit` accepts language selection; `auto` derives it from the text.
    pub language_control: &'static str,
    /// `preset`, `embedded`, `generated`, `reference_source`, or `reference_profile`.
    pub voice_mode: &'static str,
    /// Fresh independent sentences may choose different speakers; submit coherent text.
    pub coherent_input: bool,
}

impl TtsCapabilities {
    /// Never silently accept a control the selected runtime does not implement.
    pub fn validate_language_control(&self, requested: Option<&str>) -> Result<()> {
        if self.language_control == "auto" && requested.is_some_and(|l| !l.trim().is_empty() && !l.eq_ignore_ascii_case("auto")) {
            anyhow::bail!("This speech runtime determines language from the text; explicit language forcing is unsupported. Omit language or use auto");
        }
        Ok(())
    }
}

pub fn discover(dir: &Path) -> Result<TtsCapabilities> {
    let voices = list_voices(dir);
    let family = FamilyAdapter::detect_family(dir, &[]);
    if family == TtsFamily::Audio8 {
        let protocol: serde_json::Value = serde_json::from_str(include_str!("families/audio8/protocol.json"))?;
        let languages = protocol["supported_languages"].as_array().into_iter().flatten()
            .filter_map(|v| v.as_str().map(str::to_owned)).collect();
        return Ok(TtsCapabilities { languages, language_control: "auto", voice_mode: "reference_profile", coherent_input: true });
    }
    let mut languages: Vec<_> = voices.iter().filter_map(|v| v.language.clone()).collect();
    if languages.is_empty() {
        languages.extend(super::g2p::declared_model_language(dir));
    }
    languages.sort();
    languages.dedup();
    Ok(TtsCapabilities { languages, language_control: "explicit", voice_mode: if voices.is_empty() { "embedded" } else { "preset" }, coherent_input: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_language_is_independent_of_named_voices() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("config.json"), r#"{"espeak":{"voice":"hi"}}"#)?;
        let caps = discover(dir.path())?;
        assert_eq!(caps.languages, ["hi"]);
        assert_eq!(caps.voice_mode, "embedded");
        std::fs::write(dir.path().join("config.json"), r#"{"tts_family":"audio8"}"#)?;
        let caps = discover(dir.path())?;
        assert_eq!(caps.languages.len(), 11);
        assert_eq!(caps.language_control, "auto");
        assert!(caps.coherent_input);
        assert!(caps.validate_language_control(Some("en")).is_err());
        assert!(caps.validate_language_control(Some("auto")).is_ok());
        assert!(caps.validate_language_control(None).is_ok());
        assert!(list_voices(dir.path()).is_empty());
        Ok(())
    }
}
