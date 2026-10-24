//! "Allow this tool for this chat": after the user approves a tool call with `remember: "chat"`,
//! later calls of the same tool in the same chat run without asking, until the grant runs out.
//! Kept in RAM only, so a restart asks again.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Longest a grant lasts
pub const MAX_GRANT_MINUTES: u64 = 24 * 60;
/// Grant length when the approval does not say
pub const DEFAULT_GRANT_MINUTES: u64 = 60;

#[derive(Default)]
pub struct Grants {
    until: HashMap<(String, String), Instant>,
}

impl Grants {
    /// Allows `tool` in `chat` for `minutes` (clamped to 1..=MAX_GRANT_MINUTES); returns the minutes
    pub fn grant(&mut self, chat: &str, tool: &str, minutes: Option<u64>, now: Instant) -> u64 {
        let minutes = minutes.unwrap_or(DEFAULT_GRANT_MINUTES).clamp(1, MAX_GRANT_MINUTES);
        self.until.insert((chat.to_string(), tool.to_string()), now + Duration::from_secs(minutes * 60));
        minutes
    }

    /// Whether the grant exists and has not run out (an expired one is removed)
    pub fn allowed(&mut self, chat: &str, tool: &str, now: Instant) -> bool {
        let key = (chat.to_string(), tool.to_string());
        match self.until.get(&key) {
            Some(until) if *until > now => true,
            Some(_) => {
                self.until.remove(&key);
                false
            }
            None => false,
        }
    }

    /// Removes the chat's grants (all, or one tool's); returns how many
    pub fn revoke(&mut self, chat: &str, tool: Option<&str>) -> usize {
        let before = self.until.len();
        self.until.retain(|(c, t), _| c != chat || tool.is_some_and(|tool| tool != t));
        before - self.until.len()
    }

    /// The chat's running grants: (tool, minutes left, rounded up)
    pub fn of(&self, chat: &str, now: Instant) -> Vec<(String, u64)> {
        let mut out: Vec<(String, u64)> = self
            .until
            .iter()
            .filter(|((c, _), until)| c == chat && **until > now)
            .map(|((_, tool), until)| (tool.clone(), until.duration_since(now).as_secs().div_ceil(60)))
            .collect();
        out.sort();
        out
    }
}

/// The Engine's grants
pub static GRANTS: LazyLock<Mutex<Grants>> = LazyLock::new(Default::default);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grants_are_per_chat_and_tool_and_run_out() {
        let mut grants = Grants::default();
        let now = Instant::now();
        assert_eq!(grants.grant("c1", "execute_code", Some(30), now), 30);
        assert!(grants.allowed("c1", "execute_code", now));
        assert!(!grants.allowed("c2", "execute_code", now));
        assert!(!grants.allowed("c1", "write_file", now));
        assert!(!grants.allowed("c1", "execute_code", now + Duration::from_secs(31 * 60)));
        // Removed once it ran out
        assert!(grants.of("c1", now).is_empty());
    }

    #[test]
    fn test_limits_and_revoke() {
        let mut grants = Grants::default();
        let now = Instant::now();
        assert_eq!(grants.grant("c1", "a", Some(100_000), now), MAX_GRANT_MINUTES);
        assert_eq!(grants.grant("c1", "b", Some(0), now), 1);
        assert_eq!(grants.grant("c2", "a", None, now), DEFAULT_GRANT_MINUTES);
        assert_eq!(grants.of("c1", now), [("a".to_string(), MAX_GRANT_MINUTES), ("b".to_string(), 1)]);
        assert_eq!(grants.revoke("c1", Some("b")), 1);
        assert_eq!(grants.revoke("c1", None), 1);
        assert!(grants.allowed("c2", "a", now));
    }
}
