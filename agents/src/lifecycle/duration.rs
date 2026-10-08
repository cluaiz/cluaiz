use super::session_map::SESSION_TOOL_REGISTRY;

/// Turn decrement engine that executes upon chat response completion
pub struct TurnLifecycleEngine;

impl TurnLifecycleEngine {
    /// Decrements turns for all active tools in a session and drops tools reaching 0
    pub fn decrement_turns(session_id: &str) {
        let mut registry = SESSION_TOOL_REGISTRY.write().unwrap();
        if let Some(bindings) = registry.get_mut(session_id) {
            let mut updated = Vec::new();
            for mut tool in bindings.drain(..) {
                if tool.turns == -1 {
                    // Persistent (All-time): keep as is
                    updated.push(tool);
                } else if tool.turns > 1 {
                    // Countdown decrement
                    tool.turns -= 1;
                    updated.push(tool);
                }
                // If tool.turns was 0 or 1, it has expired and is auto-unloaded (dropped)
            }
            *bindings = updated;
        }
    }
}
