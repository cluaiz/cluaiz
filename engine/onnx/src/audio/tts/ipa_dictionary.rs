//! Pronunciation dictionaries for phoneme TTS models (word → IPA).
//!
//! They are never shipped with the engine. The first time a language is needed, its file is
//! downloaded from the source at the commit pinned in `data/lexicon_sources.json`, checked against
//! its size and SHA-256, and compiled to `<cluaiz home>/data/lexicons/<file>-<sha>.bin`. Later loads
//! read that file without the network. Languages whose license does not allow use in Cluaiz are
//! marked `enabled: false` there and are never fetched.

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SOURCES_JSON: &str = include_str!("data/lexicon_sources.json");
/// Dictionary files the engine itself used to write into model folders (copied or compiled lexicons)
const ENGINE_WRITTEN_FILES: [&str; 2] = ["lexicon.txt", "lexicon.bin"];

#[derive(Deserialize)]
struct Sources {
    commit: String,
    url: String,
    languages: Vec<Source>,
}

#[derive(Deserialize)]
struct Source {
    file: String,
    codes: Vec<String>,
    bytes: u64,
    sha256: String,
    enabled: bool,
}

fn sources() -> Option<&'static Sources> {
    static SOURCES: OnceLock<Option<Sources>> = OnceLock::new();
    SOURCES
        .get_or_init(|| match serde_json::from_str(SOURCES_JSON) {
            Ok(sources) => Some(sources),
            Err(e) => {
                tracing::error!("📖 [Lexicon] lexicon_sources.json is invalid: {e}");
                None
            }
        })
        .as_ref()
}

/// The dictionary for a model language: its exact code first ("en-us"), then the base language
/// ("en-in" → "en"). Only languages whose license allows use are returned.
fn source_for<'a>(sources: &'a Sources, lang: &str) -> Option<&'a Source> {
    let lang = lang.trim().to_lowercase().replace('_', "-");
    let base = lang.split('-').next().unwrap_or(&lang).to_owned();
    [lang, base].iter().find_map(|code| {
        sources
            .languages
            .iter()
            .find(|s| s.enabled && s.codes.iter().any(|c| c == code))
    })
}

/// Where downloaded dictionaries live.
pub fn lexicon_dir() -> PathBuf {
    cluaiz_core::EnvironmentManager::current()
        .global_dir
        .join("data")
        .join("lexicons")
}

fn bin_path(dir: &Path, source: &Source) -> PathBuf {
    let stem = source.file.trim_end_matches(".txt");
    let sha = source.sha256.get(..8).unwrap_or(&source.sha256);
    dir.join(format!("{stem}-{sha}.bin"))
}

/// The pronunciation dictionary for a model language, downloaded on first use. Empty when the
/// language has no usable dictionary or the download failed (the caller falls back to its rules).
pub fn load_language_lexicon(lang: &str) -> HashMap<String, String> {
    let Some(sources) = sources() else {
        return HashMap::new();
    };
    let Some(source) = source_for(sources, lang) else {
        tracing::info!("📖 [Lexicon] No pronunciation dictionary for language '{lang}'");
        return HashMap::new();
    };
    let dir = lexicon_dir();
    let bin = bin_path(&dir, source);
    if !bin.exists() {
        if let Err(e) = fetch(sources, source, &dir, &bin) {
            tracing::warn!("📖 [Lexicon] Could not get the '{}' dictionary: {e:#}", source.file);
            return HashMap::new();
        }
    }
    match load_bin_lexicon(&bin) {
        Ok(dict) => {
            tracing::info!("📖 [Lexicon] Loaded {} words for '{lang}' from {}", dict.len(), source.file);
            dict
        }
        Err(e) => {
            // A damaged file is removed so the next load downloads it again
            tracing::warn!("📖 [Lexicon] {:?} is damaged ({e}); it will be downloaded again", bin);
            let _ = std::fs::remove_file(&bin);
            HashMap::new()
        }
    }
}

/// Downloads one dictionary at the pinned commit, verifies it and writes the compiled file. Nothing
/// is left behind on failure; the compiled file appears only complete (written aside, then renamed).
fn fetch(sources: &Sources, source: &Source, dir: &Path, bin: &Path) -> Result<()> {
    let url = sources
        .url
        .replace("{commit}", &sources.commit)
        .replace("{file}", &source.file);
    tracing::info!("📖 [Lexicon] Downloading {} ({} bytes)", source.file, source.bytes);
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("Cluaiz-Engine")
        .build()?
        .get(&url)
        .send()
        .with_context(|| format!("request to {url} failed"))?
        .error_for_status()?;
    let mut body = Vec::with_capacity(source.bytes as usize);
    response.take(source.bytes + 1).read_to_end(&mut body)?;
    if body.len() as u64 != source.bytes {
        bail!("expected {} bytes, got {}", source.bytes, body.len());
    }
    let sha = format!("{:x}", Sha256::digest(&body));
    if sha != source.sha256 {
        bail!("SHA-256 mismatch (expected {}, got {sha})", source.sha256);
    }
    let text = String::from_utf8(body).map_err(|_| anyhow!("file is not UTF-8"))?;
    let entries = parse_source(&text);
    if entries.is_empty() {
        bail!("file has no entries");
    }

    std::fs::create_dir_all(dir)?;
    let part = bin.with_extension(format!("part{}", std::process::id()));
    if let Err(e) = write_bin(&entries, &part).and_then(|()| std::fs::rename(&part, bin)) {
        let _ = std::fs::remove_file(&part);
        return Err(e.into());
    }
    remove_older_versions(dir, source, bin);
    tracing::info!("📖 [Lexicon] Saved {} words to {:?}", entries.len(), bin);
    Ok(())
}

/// Compiled files of the same dictionary from an earlier pinned version.
fn remove_older_versions(dir: &Path, source: &Source, current: &Path) {
    let prefix = format!("{}-", source.file.trim_end_matches(".txt"));
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path != current && name.starts_with(&prefix) && name.ends_with(".bin") {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Source format: `word<TAB>/ipa/` or `word<TAB>/ipa1/, /ipa2/`; the first pronunciation is used.
fn parse_source(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (word, ipa) = line.split_once('\t')?;
            let first = ipa.split(", ").next()?.trim().trim_matches('/').trim();
            let word = word.trim().to_lowercase();
            (!word.is_empty() && !first.is_empty()).then(|| (word, first.to_owned()))
        })
        .collect()
}

/// Binary layout: u32 count, then per entry u32 length + word bytes, u32 length + IPA bytes (LE).
fn write_bin(entries: &[(String, String)], path: &Path) -> std::io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    writer.write_all(&(entries.len() as u32).to_le_bytes())?;
    for (word, ipa) in entries {
        for bytes in [word.as_bytes(), ipa.as_bytes()] {
            writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
            writer.write_all(bytes)?;
        }
    }
    writer.flush()
}

/// Load a compiled lexicon .bin into memory
pub fn load_bin_lexicon(bin_path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
    let buf = std::fs::read(bin_path)?;
    let mut cursor = 0;
    let count = read_u32(&buf, &mut cursor)? as usize;
    let mut dict = HashMap::with_capacity(count);
    for _ in 0..count {
        let word = read_text(&buf, &mut cursor)?;
        let ipa = read_text(&buf, &mut cursor)?;
        dict.insert(word, ipa);
    }
    Ok(dict)
}

fn invalid(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_owned())
}

fn read_u32(buf: &[u8], cursor: &mut usize) -> Result<u32, std::io::Error> {
    let bytes: [u8; 4] = buf
        .get(*cursor..*cursor + 4)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| invalid("truncated lexicon"))?;
    *cursor += 4;
    Ok(u32::from_le_bytes(bytes))
}

fn read_text(buf: &[u8], cursor: &mut usize) -> Result<String, std::io::Error> {
    let len = read_u32(buf, cursor)? as usize;
    let text = buf
        .get(*cursor..*cursor + len)
        .ok_or_else(|| invalid("truncated lexicon"))?;
    *cursor += len;
    String::from_utf8(text.to_vec()).map_err(|_| invalid("lexicon is not UTF-8"))
}

/// Earlier engine versions copied a shared dictionary into the model folder as `lexicon.txt` and
/// compiled `lexicon.bin` next to it; those copies hide the model's own data and the current
/// dictionaries. Files the model did not ship (per its `hf_metadata.json` file list) are moved to
/// `<cluaiz home>/backup/tts-lexicons/<model>/`, never deleted. Without a file list only the
/// compiled `lexicon.bin` (always engine-written) is moved.
pub fn retire_engine_lexicons(model_dir: &Path) {
    let shipped: Option<Vec<String>> = std::fs::read(model_dir.join("hf_metadata.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|meta| {
            meta.get("siblings")?.as_array().map(|files| {
                files
                    .iter()
                    .filter_map(|f| f.get("rfilename")?.as_str())
                    .filter_map(|f| f.rsplit('/').next())
                    .map(str::to_owned)
                    .collect()
            })
        });
    for name in ENGINE_WRITTEN_FILES {
        let path = model_dir.join(name);
        if !path.exists() {
            continue;
        }
        let engine_written = match &shipped {
            Some(files) => !files.iter().any(|f| f == name),
            None => name.ends_with(".bin"),
        };
        if !engine_written {
            continue;
        }
        let model = model_dir.file_name().unwrap_or_default();
        let backup = cluaiz_core::EnvironmentManager::current()
            .global_dir
            .join("backup")
            .join("tts-lexicons")
            .join(model);
        let target = backup.join(name);
        let moved = std::fs::create_dir_all(&backup)
            .and_then(|()| std::fs::rename(&path, &target))
            .or_else(|_| std::fs::copy(&path, &target).and_then(|_| std::fs::remove_file(&path)));
        match moved {
            Ok(()) => tracing::info!("📖 [Lexicon] Moved engine-written {:?} to {:?}", path, target),
            Err(e) => tracing::warn!("📖 [Lexicon] Could not move {:?} to the backup: {e}", path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_parse_and_every_language_is_pinned() {
        let sources = sources().expect("lexicon_sources.json parses");
        assert_eq!(sources.commit.len(), 40);
        for s in &sources.languages {
            assert_eq!(s.sha256.len(), 64, "{}", s.file);
            assert!(s.bytes > 0 && !s.codes.is_empty(), "{}", s.file);
        }
    }

    #[test]
    fn language_codes_resolve_with_base_fallback_and_license_gate() {
        let sources = sources().unwrap();
        assert_eq!(source_for(sources, "en-us").unwrap().file, "en_US.txt");
        assert_eq!(source_for(sources, "en_IN").unwrap().file, "en_US.txt");
        assert_eq!(source_for(sources, "fr-fr").unwrap().file, "fr_FR.txt");
        // GPL and non-commercial dictionaries are never used
        assert_eq!(source_for(sources, "en-gb").unwrap().file, "en_US.txt");
        assert!(source_for(sources, "ro").is_none());
        assert!(source_for(sources, "hi").is_none());
    }

    #[test]
    fn source_lines_keep_the_first_pronunciation() {
        let entries = parse_source("A\t/ˈeɪ/, /ə/\n---\nabbe\t/ˈæbi/\n");
        assert_eq!(entries, vec![("a".into(), "ˈeɪ".into()), ("abbe".into(), "ˈæbi".into())]);
    }

    #[test]
    fn compiled_file_round_trips() {
        let path = std::env::temp_dir().join(format!("lexicon-test-{}.bin", std::process::id()));
        let entries = vec![("hello".to_owned(), "həˈloʊ".to_owned())];
        write_bin(&entries, &path).unwrap();
        let dict = load_bin_lexicon(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(dict.get("hello").map(String::as_str), Some("həˈloʊ"));
    }
}
