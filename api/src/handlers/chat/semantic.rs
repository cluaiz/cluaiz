//! Meaning-based matching of notes and project facts, with an installed embedding model.
//!
//! Which model: the policy's `notes_match` ("auto": the installed embedding model, the first by id
//! when there are several; an id: that model; "words": none). Each note is embedded once and kept
//! (`cluaiz_agents::memory::vectors`); a message costs one embedding. Whenever the model is missing,
//! busy or fails, the callers fall back to word matching, so memory never stops working.

use crate::state::AppState;
use cluaiz_agents::memory::vectors::{self, VectorStore};
use cluaiz_models::types::manifest::ModelRegistryEntry;

/// The embedding model notes are matched with, if any
pub fn embedding_model(policy: &cluaiz_context::Policy) -> Option<ModelRegistryEntry> {
    let choice = policy.notes_match.trim();
    if choice.eq_ignore_ascii_case("words") {
        return None;
    }
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    let usable = |entry: &&ModelRegistryEntry| crate::handlers::embeddings::is_embedding_model(entry) && entry.primary_path().is_some_and(|p| p.is_file());
    if !choice.is_empty() && !choice.eq_ignore_ascii_case("auto") {
        return registry.installed_models.get(choice).filter(usable).cloned();
    }
    let mut found: Vec<&ModelRegistryEntry> = registry.installed_models.values().filter(usable).collect();
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found.first().map(|entry| (*entry).clone())
}

fn model_key(entry: &ModelRegistryEntry) -> String {
    let bytes = entry.primary_path().and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
    vectors::model_key(&entry.id, bytes)
}

/// The saved context policy, for a caller without a request (the terminal agent)
pub fn saved_policy() -> cluaiz_context::Policy {
    super::context_plan::policy(None)
}

/// [`similarities_with`] on this Engine's own hosts
pub async fn similarities(
    state: &AppState,
    policy: &cluaiz_context::Policy,
    query: &str,
    items: &[(String, String)],
    live: Option<&std::collections::HashSet<String>>,
) -> Option<Vec<f32>> {
    let host = state.host.clone();
    similarities_with(policy, query, items, live, |entry, texts| async move {
        crate::handlers::embeddings::embed_texts(&host, &entry, texts).await
    })
    .await
}

/// Similarity (cosine) of `query` to each `(key, text)`, embedding only texts not kept yet with
/// `embed` (this process's hosts, or a running server). None when there is no embedding model or
/// it could not run (the caller matches by words).
/// `live`, when given, are all the keys still in use: vectors of other keys are dropped.
pub async fn similarities_with<F, Fut>(
    policy: &cluaiz_context::Policy,
    query: &str,
    items: &[(String, String)],
    live: Option<&std::collections::HashSet<String>>,
    embed: F,
) -> Option<Vec<f32>>
where
    F: FnOnce(ModelRegistryEntry, Vec<String>) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Vec<Vec<f32>>>>,
{
    let entry = embedding_model(policy)?;
    let key = model_key(&entry);
    let store = VectorStore::shared();
    let mut kept = store.cached(&key, items);
    let missing: Vec<usize> = kept.iter().enumerate().filter(|(_, v)| v.is_none()).map(|(i, _)| i).collect();
    let mut texts: Vec<String> = missing.iter().map(|&i| items[i].1.clone()).collect();
    texts.push(query.to_string());
    let started = std::time::Instant::now();
    let mut made = match embed(entry.clone(), texts).await {
        Ok(made) => made,
        Err(e) => {
            tracing::warn!("⚠️ [Notes] Embedding model {} not used ({:#}); matching by words", entry.id, e);
            return None;
        }
    };
    let query_vector = made.pop()?;
    let new: Vec<(String, String, Vec<f32>)> =
        missing.iter().zip(made).map(|(&i, vector)| (items[i].0.clone(), items[i].1.clone(), vector)).collect();
    for (i, (_, _, vector)) in missing.iter().zip(&new) {
        kept[*i] = Some(vector.clone());
    }
    if !new.is_empty() || live.is_some() {
        if let Err(e) = store.store(&key, new, live) {
            tracing::warn!("⚠️ [Notes] Could not keep note vectors: {}", e);
        }
    }
    tracing::debug!("🧭 [Notes] {} texts matched with {} ({} embedded) in {} ms", items.len(), entry.id, missing.len(), started.elapsed().as_millis());
    Some(kept.iter().map(|v| v.as_deref().map_or(0.0, |v| vectors::cosine(&query_vector, v))).collect())
}

/// The items with a similarity from `floor_pct`, most alike first, at most `limit`
pub fn top<T>(items: Vec<T>, scores: &[f32], floor_pct: u8, limit: usize) -> Vec<T> {
    let mut scored: Vec<(f32, T)> = scores.iter().copied().zip(items).filter(|(score, _)| *score * 100.0 >= floor_pct as f32).collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(limit).map(|(_, item)| item).collect()
}

/// The project's live facts that may be about the same thing as `text`, by meaning, most alike
/// first; word matching when there is no embedding model or it could not run. A project without
/// facts loads no model. `name` is the new fact's name, if it has one: a fact of that name counts
/// whatever its words are.
pub async fn similar_facts_with<'a, F, Fut>(
    policy: &cluaiz_context::Policy,
    workspace: &'a cluaiz_agents::workspace::Workspace,
    name: &str,
    text: &str,
    limit: usize,
    embed: F,
) -> Vec<&'a cluaiz_agents::workspace::Item>
where
    F: FnOnce(ModelRegistryEntry, Vec<String>) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Vec<Vec<f32>>>>,
{
    use cluaiz_agents::memory::notes::FACT_PREFIX;
    use cluaiz_agents::workspace::memory;
    let facts: Vec<&cluaiz_agents::workspace::Item> = workspace.memory.iter().filter(|item| item.superseded_by.is_none()).collect();
    if facts.is_empty() {
        return Vec::new();
    }
    let items: Vec<(String, String)> =
        facts.iter().map(|item| (format!("{FACT_PREFIX}{}", item.path.display()), item.body().unwrap_or_default().trim().to_string())).collect();
    match similarities_with(policy, text, &items, None, embed).await {
        Some(scores) => {
            let mut chosen = top(facts, &scores, policy.notes_similar_pct, limit);
            if let Some(same) = (!name.is_empty()).then(|| memory::live(workspace, name)).flatten() {
                chosen.retain(|item| !std::ptr::eq(*item, same));
                chosen.insert(0, same);
                chosen.truncate(limit);
            }
            chosen
        }
        None => memory::similar(workspace, name, text, limit),
    }
}

/// Deletes the kept vectors of an embedding model that is removed
pub fn forget_model(model_id: &str) {
    let removed = VectorStore::shared().forget_model(model_id);
    if removed > 0 {
        tracing::info!("🧹 [Notes] Dropped {} vector file(s) of the removed model '{}'", removed, model_id);
    }
}
