//! Which file a model name means on this machine: a path to a weight file, or an installed model id
//! (the same registry lookup the API uses for a request's `model`).

use cluaiz_models::registry::InstalledStateRegistry;
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

/// Like `lookup`, but only installed models count; the error says what to do instead.
pub fn resolve_installed(name: &str) -> color_eyre::Result<(PathBuf, String)> {
    match lookup(name) {
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
