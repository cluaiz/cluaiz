//! Chat memory on disk: llama-server's `--slot-save-path`, managed per chat by the engine.
//! After an answer the chat's KV cache is written to `kv_cache/chats/<chat>.kv` (+ media layout and
//! a JSON record); when that chat continues after another chat, a model reload or a restart, the
//! KV is read back instead of processing the whole history again. Records carry the exact model
//! and context they were made with, so a different model or context never restores them.

use cluaiz_core::{ChatMemorySettings, EnvironmentManager};
use llama_engine::native::session::NativeSession;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// One saved chat, as listed by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMemoryRecord {
    pub chat_id: String,
    /// Model file the KV belongs to
    pub model: String,
    /// Exact model file + load settings + KV type; must match to restore
    pub key: String,
    /// Text tokens held (0 for a chat with images or audio, which keeps a chunk layout)
    pub tokens: usize,
    /// KV positions the chat fills; a context at least this large can restore it
    #[serde(default)]
    pub positions: usize,
    /// Disk used by the chat's files
    pub bytes: u64,
    /// Unix seconds
    pub saved_at: u64,
    pub last_used: u64,
}

/// Disk used by chat memory and the saved chats, newest use first.
#[derive(Debug, Clone, Serialize)]
pub struct ChatMemoryUsage {
    pub bytes: u64,
    pub chats: Vec<ChatMemoryRecord>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Where chat memory lives (`<cluaiz home>/kv_cache/chats`).
pub fn directory() -> PathBuf {
    EnvironmentManager::current().kv_cache_dir().join("chats")
}

/// File stem for a chat id: the id itself when it is a plain name, else its FNV-1a hash.
fn stem(chat_id: &str) -> String {
    let plain = !chat_id.is_empty()
        && chat_id.len() <= 64
        && chat_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        return chat_id.to_owned();
    }
    let hash = chat_id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("chat-{hash:016x}")
}

fn files(base: &Path) -> [PathBuf; 3] {
    [
        base.with_extension("kv"),
        base.with_extension("layout.json"),
        base.with_extension("json"),
    ]
}

fn read_record(base: &Path) -> Option<ChatMemoryRecord> {
    serde_json::from_slice(&std::fs::read(base.with_extension("json")).ok()?).ok()
}

/// The identity a saved KV must match: model file (path, size, modification time), the load
/// settings fingerprint and the KV type llama.cpp created. The context size may differ (Auto sizes it
/// by free memory): llama.cpp restores a sequence into any context with room for its positions.
pub fn memory_key(load_key: &str, model_path: &str, kv_type: &str) -> String {
    let file = std::fs::metadata(model_path)
        .map(|m| {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("{}:{}", m.len(), modified)
        })
        .unwrap_or_default();
    format!("{load_key}|file={file}|kv={kv_type}")
}

/// Saves the chat's KV, then keeps the folder within the age and disk limits.
pub fn save(
    session: &NativeSession,
    chat_id: &str,
    tokens: &[i32],
    positions: usize,
    key: &str,
    model: &str,
    settings: &ChatMemorySettings,
) -> anyhow::Result<u64> {
    let dir = directory();
    std::fs::create_dir_all(&dir)?;
    let base = dir.join(stem(chat_id));
    session.save_memory(&base, tokens)?;
    let bytes = files(&base)
        .iter()
        .filter_map(|f| std::fs::metadata(f).ok())
        .map(|m| m.len())
        .sum();
    let time = now();
    let record = ChatMemoryRecord {
        chat_id: chat_id.to_owned(),
        model: model.to_owned(),
        key: key.to_owned(),
        tokens: tokens.len(),
        positions,
        bytes,
        saved_at: time,
        last_used: time,
    };
    std::fs::write(
        base.with_extension("json"),
        serde_json::to_vec_pretty(&record)?,
    )?;
    enforce_limits(settings, Some(chat_id));
    Ok(bytes)
}

/// Restores the chat's KV when a record for exactly this model exists and fits the context. Returns
/// its text token list, or None when there is nothing to restore (the history is processed instead).
pub fn restore(
    session: &mut NativeSession,
    chat_id: &str,
    key: &str,
) -> anyhow::Result<Option<Vec<i32>>> {
    let base = directory().join(stem(chat_id));
    let Some(mut record) = read_record(&base) else {
        return Ok(None);
    };
    if record.chat_id != chat_id
        || record.key != key
        || record.positions >= session.context_tokens() as usize
    {
        return Ok(None);
    }
    let tokens = session.restore_memory(&base)?;
    record.last_used = now();
    std::fs::write(
        base.with_extension("json"),
        serde_json::to_vec_pretty(&record)?,
    )?;
    Ok(Some(tokens))
}

/// Saved chats and the disk they use.
pub fn usage() -> ChatMemoryUsage {
    let mut chats: Vec<ChatMemoryRecord> = std::fs::read_dir(directory())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json")
                && !p.to_string_lossy().ends_with(".layout.json")
        })
        .filter_map(|p| read_record(&p.with_extension("")))
        .collect();
    chats.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    ChatMemoryUsage {
        bytes: chats.iter().map(|c| c.bytes).sum(),
        chats,
    }
}

/// Deletes one chat's memory; true when something was deleted.
pub fn delete(chat_id: &str) -> bool {
    let base = directory().join(stem(chat_id));
    files(&base)
        .iter()
        .map(|f| std::fs::remove_file(f).is_ok())
        .fold(false, |any, removed| any || removed)
}

/// Deletes every saved chat; returns how many.
pub fn clear() -> usize {
    usage().chats.iter().filter(|c| delete(&c.chat_id)).count()
}

/// Deletes chats unused for longer than `max_age_days`, then the least recently used ones until the
/// folder fits `max_disk_gb`. `keep` (the chat just saved) goes last.
pub fn enforce_limits(settings: &ChatMemorySettings, keep: Option<&str>) {
    let mut chats = usage().chats;
    if settings.max_age_days > 0 {
        let oldest = now().saturating_sub(u64::from(settings.max_age_days) * 86_400);
        chats.retain(|c| {
            let expired = c.last_used < oldest && Some(c.chat_id.as_str()) != keep;
            if expired {
                delete(&c.chat_id);
            }
            !expired
        });
    }
    let limit = (settings.max_disk_gb.max(0.0) * cluaiz_core::GIB_F64) as u64;
    let mut total: u64 = chats.iter().map(|c| c.bytes).sum();
    // usage() lists newest first; evict from the end, the just-saved chat only if alone over the limit
    chats.sort_by_key(|c| (Some(c.chat_id.as_str()) == keep, c.last_used));
    for c in &chats {
        if total <= limit {
            break;
        }
        if delete(&c.chat_id) {
            tracing::info!(
                "💾 [ChatMemory] Removed saved chat {} ({} bytes) to stay within {:.1} GB",
                c.chat_id,
                c.bytes,
                settings.max_disk_gb
            );
            total = total.saturating_sub(c.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_ids_map_to_safe_file_names() {
        assert_eq!(stem("abc_DEF-123"), "abc_DEF-123");
        assert!(stem("../../etc/passwd").starts_with("chat-"));
        assert!(stem("").starts_with("chat-"));
        assert_ne!(stem("a/b"), stem("a\\b"));
    }

    #[test]
    fn key_changes_with_model_settings_and_kv_type() {
        let a = memory_key("m|s", "missing.gguf", "Kv8");
        assert_ne!(a, memory_key("m|s2", "missing.gguf", "Kv8"));
        assert_ne!(a, memory_key("m|s", "missing.gguf", "Kv16"));
    }
}
