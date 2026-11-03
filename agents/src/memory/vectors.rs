//! Embeddings of notes and project facts, made once and kept: a note is embedded when it is new or
//! its text changed, never again for each message. One file per embedding model,
//! `<CLUAIZ_HOME>/memory/vectors/<model>.json`, read into RAM on first use; a vector is keyed by the
//! note id (or `fact:<path>`) and the hash of the text it was made from. Another embedding model,
//! or a new file of the same model, has its own file; removing the model deletes it
//! ([`forget_model`]).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    hash: u64,
    vector: Vec<f32>,
}

#[derive(Default, Serialize, Deserialize)]
struct Vectors {
    entries: HashMap<String, Entry>,
}

/// Vector files read into RAM, by file path
static CACHE: LazyLock<Mutex<HashMap<PathBuf, Vectors>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The folder of the vector files: `<CLUAIZ_HOME>/memory/vectors`, or another one (tests)
#[derive(Clone)]
pub struct VectorStore {
    dir: PathBuf,
}

/// File-name safe form of a model key
fn file_stem(model_key: &str) -> String {
    model_key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}


/// What identifies an embedding model's vectors: its id and the size of its file (a new file of
/// the same id makes other vectors)
pub fn model_key(model_id: &str, file_bytes: u64) -> String {
    format!("{model_id}@{file_bytes}")
}

/// A stable hash of a text (FNV-1a), so a vector is reused only for the text it was made from
pub fn text_hash(text: &str) -> u64 {
    text.trim().bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x0100_0000_01b3))
}

impl VectorStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The Engine's vector files
    pub fn shared() -> Self {
        Self::new(super::memory_root().join("vectors"))
    }

    fn file(&self, model_key: &str) -> PathBuf {
        self.dir.join(format!("{}.json", file_stem(model_key)))
    }

    fn with_model<T>(&self, model_key: &str, f: impl FnOnce(&mut Vectors) -> T) -> T {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.file(model_key);
        let vectors = cache.entry(path.clone()).or_insert_with(|| {
            std::fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
        });
        f(vectors)
    }

    /// The kept vector of each `(key, text)`, None where there is none for that text yet
    pub fn cached(&self, model_key: &str, items: &[(String, String)]) -> Vec<Option<Vec<f32>>> {
        self.with_model(model_key, |vectors| {
            items
                .iter()
                .map(|(key, text)| vectors.entries.get(key).filter(|e| e.hash == text_hash(text)).map(|e| e.vector.clone()))
                .collect()
        })
    }

    /// Keeps new vectors and, when `live` is given, drops the vectors of notes (keys `n:…`) not in
    /// it (a fact's vector is dropped with its text change). Writes the file only when it changed.
    pub fn store(&self, model_key: &str, new: Vec<(String, String, Vec<f32>)>, live: Option<&HashSet<String>>) -> std::io::Result<()> {
        let text = self.with_model(model_key, |vectors| {
            let mut changed = !new.is_empty();
            for (key, text, vector) in new {
                vectors.entries.insert(key, Entry { hash: text_hash(&text), vector });
            }
            if let Some(live) = live {
                let before = vectors.entries.len();
                vectors.entries.retain(|key, _| !key.starts_with("n:") || live.contains(key));
                changed |= vectors.entries.len() != before;
            }
            changed.then(|| serde_json::to_string(vectors).unwrap_or_default())
        });
        let Some(text) = text else { return Ok(()) };
        std::fs::create_dir_all(&self.dir)?;
        let path = self.file(model_key);
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(temporary, path)
    }

    /// Deletes every vector file of an embedding model (it was removed); returns how many
    pub fn forget_model(&self, model_id: &str) -> usize {
        let prefix = file_stem(&format!("{model_id}@"));
        let starts = |path: &PathBuf| path.parent() == Some(self.dir.as_path()) && path.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix));
        CACHE.lock().unwrap_or_else(|e| e.into_inner()).retain(|path, _| !starts(path));
        std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|f| f.file_name().to_string_lossy().starts_with(&prefix))
            .filter(|f| std::fs::remove_file(f.path()).is_ok())
            .count()
    }
}

/// Cosine similarity of two vectors (0 when either is empty or of another size)
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors_are_kept_per_text_and_dropped_with_their_model() {
        let dir = std::env::temp_dir().join(format!("cluaiz-vectors-{}", uuid::Uuid::new_v4().simple()));
        let store = VectorStore::new(dir.clone());
        let key = model_key("test-embed", 123);
        let items = vec![("n:1".to_string(), "Port 9090".to_string()), ("n:2".to_string(), "Tea at 4".to_string())];
        assert_eq!(store.cached(&key, &items), vec![None, None]);
        store.store(&key, vec![("n:1".into(), "Port 9090".into(), vec![1.0, 0.0])], None).unwrap();
        assert_eq!(store.cached(&key, &items), vec![Some(vec![1.0, 0.0]), None]);
        // A changed text needs a new vector
        assert_eq!(store.cached(&key, &[("n:1".into(), "Port 8080".into())]), vec![None]);
        // Notes that are gone leave the file
        store.store(&key, vec![("n:2".into(), "Tea at 4".into(), vec![0.0, 1.0])], Some(&HashSet::from(["n:2".to_string()]))).unwrap();
        assert_eq!(store.cached(&key, &items), vec![None, Some(vec![0.0, 1.0])]);
        assert!(store.file(&key).exists());
        let id = key.split('@').next().unwrap().to_string();
        assert_eq!(store.forget_model(&id), 1);
        assert!(!store.file(&key).exists());
        let _ = std::fs::remove_dir_all(dir);
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert_eq!(cosine(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
        assert_eq!(text_hash("a b"), text_hash(" a b "));
    }
}
