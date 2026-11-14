//! Which file a model name means on this machine: a path to a weight file, or an installed model id
//! (the same registry lookup the API uses for a request's `model`).

use cluaiz_models::registry::InstalledStateRegistry;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub enum Lookup {
    /// (weight file, id: the registry id or the file name)
    Found(PathBuf, String),
    /// More than one installed id contains the name
    Ambiguous(Vec<String>),
    NotInstalled,
}

pub fn lookup(name: &str) -> Lookup {
    let as_path = Path::new(name);
    if as_path.is_file() {
        return Lookup::Found(as_path.to_path_buf(), InstalledStateRegistry::id_for_path(as_path));
    }
    let registry = InstalledStateRegistry::load();
    match registry.find(name).as_slice() {
        [] => Lookup::NotInstalled,
        [entry] => match entry.primary_path() {
            Some(path) => Lookup::Found(path, entry.id.clone()),
            None => Lookup::NotInstalled,
        },
        many => Lookup::Ambiguous(many.iter().map(|e| e.id.clone()).collect()),
    }
}

/// An installed model by the repository name `pull` and `run` take: `owner/repo`, or
/// `owner/repo:QUANT` for one variant. Read from the registry only, with no network.
fn lookup_repository(name: &str) -> Lookup {
    let (repo, quant) = match name.trim().rsplit_once(':') {
        Some((repo, quant)) if repo.contains('/') => (repo, Some(quant)),
        _ => (name.trim(), None),
    };
    if !repo.contains('/') {
        return Lookup::NotInstalled;
    }
    let norm = |s: &str| s.to_lowercase().replace(['-', '_', '.', ' '], "");
    let registry = InstalledStateRegistry::load();
    let mut found: Vec<_> = registry
        .installed_models
        .values()
        .filter(|entry| entry.huggingface_repo.eq_ignore_ascii_case(repo))
        .filter(|entry| quant.is_none_or(|quant| norm(&entry.id).ends_with(&norm(quant))))
        .filter_map(|entry| entry.primary_path().map(|path| (path, entry.id.clone())))
        .collect();
    match found.len() {
        0 => Lookup::NotInstalled,
        1 => {
            let (path, id) = found.remove(0);
            Lookup::Found(path, id)
        }
        _ => {
            let mut ids: Vec<String> = found.into_iter().map(|(_, id)| id).collect();
            ids.sort();
            Lookup::Ambiguous(ids)
        }
    }
}

/// Like `lookup`, but only installed models count, also by repository name; the error says what to
/// do instead.
pub fn resolve_installed(name: &str) -> color_eyre::Result<(PathBuf, String)> {
    let found = match lookup(name) {
        Lookup::NotInstalled => lookup_repository(name),
        other => other,
    };
    match found {
        Lookup::Found(path, id) => Ok((path, id)),
        Lookup::Ambiguous(ids) => Err(color_eyre::eyre::eyre!(
            "'{}' matches several installed models: {}. Use the full id.", name, ids.join(", ")
        )),
        Lookup::NotInstalled => Err(color_eyre::eyre::eyre!(
            "model '{}' is not installed (see `cluaiz list`, or `cluaiz pull {}`)", name, name
        )),
    }
}

/// The model `cluaiz run` uses when none is given: the active chat model in permission.json.
pub fn default_chat_model() -> Option<String> {
    cluaiz_security::PermissionSchema::load().get_active_chat_model()
}

/// The chat model when none is given: the active one (settings), else the only installed one, else
/// the user picks one in a list. A model chosen here becomes the active one, so it is asked once.
pub fn chat_model_or_choose() -> color_eyre::Result<String> {
    use color_eyre::eyre::eyre;
    // The default may be installed, or a provider's model (`cluaiz model set-chat groq/<model>`)
    if let Some(active) = default_chat_model().filter(|id| matches!(lookup(id), Lookup::Found(..)) || crate::chat::provider::find(id).is_some()) {
        return Ok(active);
    }
    let mut installed: Vec<String> = InstalledStateRegistry::load()
        .installed_models
        .into_values()
        .filter(|entry| entry.category == "chat" && entry.primary_path().is_some_and(|p| p.is_file()))
        .map(|entry| entry.id)
        .collect();
    installed.sort();
    let chosen = match installed.len() {
        0 => return Err(eyre!("no chat model installed yet: download one with `cluaiz pull <model>` (for example `cluaiz pull unsloth/Qwen3-4B-GGUF`)")),
        1 => installed.remove(0),
        _ if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() => {
            inquire::Select::new("Which model? (saved as the default; change it with `cluaiz model set-chat`)", installed)
                .prompt()
                .map_err(|e| eyre!("no model chosen: {e}"))?
        }
        _ => return Err(eyre!("no default chat model: pass -m <model>, or set one with `cluaiz model set-chat <id>` (installed: {})", installed.join(", "))),
    };
    cluaiz_security::PermissionSchema::set_active_chat_model(chosen.clone());
    eprintln!("Using {chosen} (now the default chat model)");
    Ok(chosen)
}
