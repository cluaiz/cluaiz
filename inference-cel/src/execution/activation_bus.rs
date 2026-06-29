//! Activation Event Bus — Tier 1.5 of the Two-Tier Registry Architecture
//!
//! A lightweight in-memory pub/sub system that fires LAZY component loading.
//! Populated at boot from `RegistryIndex.lazy` — activation_events come from the
//! manifest, NOT from engine code.
//!
//! ## Activation Event Grammar (from registry.yaml)
//!
//! | Event Format                     | Trigger Condition                                     |
//! |----------------------------------|-------------------------------------------------------|
//! | `on_startup`                     | Engine boots (used only for EAGER — ignored here)     |
//! | `on_cel_keyword:<word>`          | CEL pipeline contains `<word>` (e.g., `scrape`)       |
//! | `on_command:use plugin::<name>`  | CLI or AI issues `use plugin::` command               |
//! | `on_file_type:<ext>`             | User provides a file with extension `<ext>`           |
//! | `on_api_route:<path>`            | A specific REST API route is hit                      |
//!
//! ## Usage
//!
//! ```rust,ignore
//! // At boot:
//! let index = MasterRegistry::load_from_path(&registry_path)?;
//! let bus = ActivationEventBus::new();
//! bus.register_from_registry(&index);
//!
//! // At runtime when AI generates CEL:
//! let names = bus.fire("on_command:use plugin::web-scraper");
//! for name in names {
//!     extension_registry.load_integration(&manifest_path_for(name))?;
//! }
//! ```

use dashmap::DashMap;
use crate::execution::registry_index::RegistryIndex;

/// Validates an activation event key against the permitted grammar.
/// Prevents arbitrary string injection into the event bus (M-2 security fix).
fn is_valid_event_key(key: &str) -> bool {
    key == "on_startup"
        || key.starts_with("on_cel_keyword:")
        || key.starts_with("on_command:use plugin::")
        || key.starts_with("on_file_type:")
        || key.starts_with("on_api_route:")
}

/// The activation event pub/sub bus.
///
/// All watcher registrations come from the `RegistryIndex` (parsed from registry.yaml).
/// No activation events are hardcoded in this struct or anywhere in the engine.
pub struct ActivationEventBus {
    /// Maps an event key → list of integration names to load when that event fires.
    /// Example: `"on_command:use plugin::web-scraper"` → `["web-scraper"]`
    watchers: DashMap<String, Vec<String>>,
}

impl Default for ActivationEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivationEventBus {
    pub fn new() -> Self {
        Self {
            watchers: DashMap::new(),
        }
    }

    /// Populates the bus from the LAZY entries in a `RegistryIndex`.
    ///
    /// Called once at engine boot after `MasterRegistry::load_from_path()`.
    /// EAGER entries are excluded — they load immediately, not on event.
    pub fn register_from_registry(&self, index: &RegistryIndex) {
        for entry in &index.lazy {
            for event in &entry.activation_events {
                self.watchers
                    .entry(event.clone())
                    .or_insert_with(Vec::new)
                    .push(entry.name.clone());

                tracing::debug!(
                    "ActivationEventBus: registered '{}' on event '{}'.",
                    entry.name, event
                );
            }
        }

        tracing::info!(
            "ActivationEventBus: {} unique events registered from {} LAZY components.",
            self.watchers.len(),
            index.lazy.len()
        );
    }

    /// Manually register a single integration for a specific event.
    /// Used when a component is installed at runtime (after initial boot).
    pub fn register(&self, event: &str, name: &str) {
        self.watchers
            .entry(event.to_string())
            .or_insert_with(Vec::new)
            .push(name.to_string());

        tracing::debug!(
            "ActivationEventBus: dynamically registered '{}' on event '{}'.",
            name, event
        );
    }

    /// Fires an event and returns the list of integration names that should be loaded.
    ///
    /// The caller is responsible for actually loading those integrations via
    /// `CluaizxtensionRegistry::load_integration()`.
    ///
    /// Returns an empty `Vec` if no integrations are registered for this event.
    pub fn fire(&self, event_key: &str) -> Vec<String> {
        // M-2: Validate event key format before firing
        if !is_valid_event_key(event_key) {
            tracing::warn!(
                "ActivationEventBus: rejected invalid event key '{}'. \
                 Valid prefixes: on_startup, on_cel_keyword:, on_command:use plugin::, \
                 on_file_type:, on_api_route:",
                event_key
            );
            return Vec::new();
        }

        let names = self.watchers
            .get(event_key)
            .map(|v| v.clone())
            .unwrap_or_default();

        if names.is_empty() {
            tracing::debug!("ActivationEventBus: event '{}' fired — no watchers.", event_key);
        } else {
            tracing::info!(
                "ActivationEventBus: event '{}' fired — triggering load for: {:?}",
                event_key, names
            );
        }

        names
    }

    /// Removes all watchers for a given integration name.
    /// Called when a component is uninstalled at runtime.
    pub fn deregister(&self, name: &str) {
        let mut to_remove: Vec<String> = Vec::new();

        for mut entry in self.watchers.iter_mut() {
            entry.value_mut().retain(|n| n != name);
            if entry.value().is_empty() {
                to_remove.push(entry.key().clone());
            }
        }

        for key in to_remove {
            self.watchers.remove(&key);
        }

        tracing::debug!(
            "ActivationEventBus: deregistered all events for '{}'.",
            name
        );
    }

    /// Returns all events currently registered in the bus.
    /// Useful for diagnostics and CLI `cluaiz status` output.
    pub fn list_events(&self) -> Vec<String> {
        self.watchers.iter().map(|e| e.key().clone()).collect()
    }

    /// Returns all integration names currently registered in the bus.
    pub fn list_watchers(&self, event_key: &str) -> Vec<String> {
        self.watchers
            .get(event_key)
            .map(|v| v.clone())
            .unwrap_or_default()
    }
}
