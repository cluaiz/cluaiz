use std::collections::HashMap;

// ─── Activation Event Bus ─────────────────────────────────────────────────────
//
// A lightweight in-memory pub/sub system that registers activation events for
// LAZY-loaded components, and fires those events at runtime to trigger loading.
//
// How it works:
//   1. At engine boot, all LAZY components register their activation_events.
//      Zero binary loading happens. Only event strings are stored.
//   2. At runtime, when the AI generates a CEL instruction or a user runs a command,
//      the engine fires the relevant event key.
//   3. The bus returns which component names should now be loaded.
//   4. The engine loads those components on-demand.

#[derive(Debug, Default)]
pub struct ActivationEventBus {
    /// Maps activation event key → list of (component_name, component_type)
    /// e.g., "on_cel_keyword:scrape" → [("web-scraper", "plugin"), ("html-parser", "plugin")]
    watchers: HashMap<String, Vec<(String, String)>>,
}

impl ActivationEventBus {
    pub fn new() -> Self {
        Self {
            watchers: HashMap::new(),
        }
    }

    /// Register a component's activation events.
    /// Called at engine boot for every LAZY component. O(n*m) where n=components, m=events per component.
    /// Allocates NO binary memory — only strings.
    ///
    /// # Arguments
    /// * `event_key`      - The event key (e.g., "on_cel_keyword:scrape")
    /// * `component_name` - The component name (e.g., "web-scraper")
    /// * `component_type` - The component type (e.g., "plugin", "extension", "mcp")
    pub fn register(&mut self, event_key: &str, component_name: &str, component_type: &str) {
        self.watchers
            .entry(event_key.to_string())
            .or_default()
            .push((component_name.to_string(), component_type.to_string()));

        tracing::debug!(
            "⏰ [EventBus] Registered '{}' ({}) for event '{}'",
            component_name, component_type, event_key
        );
    }

    /// Register all activation events for a component at once.
    pub fn register_all(&mut self, events: &[String], component_name: &str, component_type: &str) {
        for event in events {
            self.register(event, component_name, component_type);
        }
    }

    /// Fire a raw event key. Returns list of (name, type) that should be loaded now.
    /// Called by the engine when a trigger condition is detected.
    pub fn fire(&self, event_key: &str) -> Vec<(String, String)> {
        match self.watchers.get(event_key) {
            Some(components) => {
                if !components.is_empty() {
                    tracing::info!(
                        "🔥 [EventBus] Event '{}' fired → {} component(s) to activate: {:?}",
                        event_key,
                        components.len(),
                        components.iter().map(|(n, _)| n).collect::<Vec<_>>()
                    );
                }
                components.clone()
            }
            None => Vec::new(),
        }
    }

    /// Fire a CEL keyword event.
    /// Called when the AI generates a CEL instruction containing a specific keyword.
    ///
    /// # Example
    /// AI generates: `use plugin::web-scraper -> scrape_url(url: "...")`
    /// Engine calls: `bus.fire_cel_keyword("scrape")` OR `bus.fire_command("use plugin::web-scraper")`
    pub fn fire_cel_keyword(&self, keyword: &str) -> Vec<(String, String)> {
        self.fire(&format!("on_cel_keyword:{}", keyword))
    }

    /// Fire a command event (CLI or AI-generated `use plugin/extension/mcp::name`).
    pub fn fire_command(&self, command: &str) -> Vec<(String, String)> {
        self.fire(&format!("on_command:{}", command))
    }

    /// Fire a file-type event (e.g., user provides a .pdf file).
    pub fn fire_file_type(&self, extension: &str) -> Vec<(String, String)> {
        self.fire(&format!("on_file_type:{}", extension))
    }

    /// Fire an API route event (e.g., a specific REST endpoint was hit).
    pub fn fire_api_route(&self, route: &str) -> Vec<(String, String)> {
        self.fire(&format!("on_api_route:{}", route))
    }

    /// Check if any component is registered for a specific event (for quick filtering).
    pub fn has_watchers_for(&self, event_key: &str) -> bool {
        self.watchers.get(event_key).map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Get total number of registered watchers across all events.
    pub fn total_watchers(&self) -> usize {
        self.watchers.values().map(|v| v.len()).sum()
    }

    /// Get all registered event keys (for debugging/status display).
    pub fn registered_events(&self) -> Vec<&String> {
        self.watchers.keys().collect()
    }
}
