//! Saved agent conversations: one JSON file per session under
//! `~/.cluaiz/sessions/<folder>-<hash>/<id>.json`, rewritten after every request, so `--continue`
//! and `--resume` can pick a conversation up again. Only text is kept: an attachment is saved as
//! a short note, not its bytes.

use std::path::{Path, PathBuf};

use cluaiz_core::{media::PreparedPart, ChatMessage};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    pub folder: String,
    pub model: String,
    pub updated: String,
    /// The first request, shortened: how the session is shown in a list
    pub title: String,
    pub messages: Vec<SavedMessage>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedMessage {
    pub role: String,
    pub text: String,
}

/// Sessions of one working folder
fn dir_for(folder: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    folder.to_string_lossy().to_lowercase().hash(&mut hasher);
    let name: String = folder
        .file_name()
        .map(|n| n.to_string_lossy().chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect())
        .unwrap_or_default();
    cluaiz_core::EnvironmentManager::current()
        .config_dir()
        .with_file_name("sessions")
        .join(format!("{}-{:08x}", name, hasher.finish() as u32))
}

/// A new session id: the start time, sortable
pub fn new_id() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn save(folder: &Path, id: &str, model: &str, history: &[ChatMessage<Vec<PreparedPart>>]) -> std::io::Result<()> {
    let messages: Vec<SavedMessage> = history
        .iter()
        .map(|message| SavedMessage {
            role: message.role.clone(),
            text: message
                .content
                .iter()
                .map(|part| match part {
                    PreparedPart::Text { text } => text.as_str(),
                    _ => "[attachment not saved]",
                })
                .collect::<Vec<_>>()
                .join("\n"),
        })
        .collect();
    // A session keeps its first title (after /compact the first message is the summary)
    let title = load(folder, Some(id))
        .map(|saved| saved.title)
        .filter(|title| !title.is_empty())
        .or_else(|| {
            messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.text.lines().next().unwrap_or_default().chars().take(60).collect())
        })
        .unwrap_or_default();
    let saved = Saved {
        id: id.to_owned(),
        folder: folder.display().to_string(),
        model: model.to_owned(),
        updated: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        title,
        messages,
    };
    let dir = dir_for(folder);
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(&saved).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(format!("{id}.json")), json)
}

/// The sessions of this folder, newest first
pub fn list(folder: &Path) -> Vec<Saved> {
    let mut all: Vec<Saved> = std::fs::read_dir(dir_for(folder))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    all.sort_by(|a, b| b.id.cmp(&a.id));
    all
}

/// `id`, or the newest session of the folder when `id` is None
pub fn load(folder: &Path, id: Option<&str>) -> Option<Saved> {
    list(folder).into_iter().find(|saved| id.is_none_or(|id| saved.id == id))
}

pub fn history(saved: &Saved) -> Vec<ChatMessage<Vec<PreparedPart>>> {
    saved
        .messages
        .iter()
        .map(|m| ChatMessage { role: m.role.clone(), content: vec![PreparedPart::Text { text: m.text.clone() }] })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_saved_listed_and_loaded_again() {
        let home = std::env::temp_dir().join(format!("cluaiz-sessions-{}", std::process::id()));
        let folder = home.join("project");
        std::fs::create_dir_all(&folder).unwrap();
        let history = vec![
            ChatMessage { role: "user".into(), content: vec![PreparedPart::Text { text: "fix calc.py\nplease".into() }] },
            ChatMessage { role: "assistant".into(), content: vec![PreparedPart::Text { text: "Done.".into() }] },
        ];
        // The saved file is plain JSON: round trip through the same functions
        let dir = dir_for(&folder);
        let _ = std::fs::remove_dir_all(&dir);
        save(&folder, "20261031-120000", "m", &history).unwrap();
        save(&folder, "20261031-130000", "m", &history[..1]).unwrap();
        let newest = load(&folder, None).unwrap();
        assert_eq!(newest.id, "20261031-130000");
        assert_eq!(newest.title, "fix calc.py");
        let older = load(&folder, Some("20261031-120000")).unwrap();
        assert_eq!(super::history(&older).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&home);
    }
}
