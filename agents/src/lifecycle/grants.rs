//! "Allow this tool for this chat": after the user approves a tool call with `remember: "chat"`,
//! later calls of the same tool in the same chat run without asking, until the grant runs out.
//! Grants and revokes are `grant` events in the chat's log, so after a restart a chat's grants
//! come back (by wall-clock time) the first time it is asked about.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Longest a grant lasts
pub const MAX_GRANT_MINUTES: u64 = 24 * 60;
/// Grant length when the approval does not say
pub const DEFAULT_GRANT_MINUTES: u64 = 60;

#[derive(Default)]
pub struct Grants {
    until: HashMap<(String, String), Instant>,
    /// Chats whose grants were read back from their logs (or that got one in this run)
    replayed: HashSet<String>,
}

/// One `grant` event of a chat's log: a grant of `minutes` for `tool`, or a revoke (of `tool`,
/// or of every tool when `tool` is None)
#[derive(Debug, Clone)]
pub struct GrantRecord {
    pub tool: Option<String>,
    pub minutes: u64,
    pub at: SystemTime,
    pub revoked: bool,
}

impl Grants {
    /// Whether the chat's grants still have to be read back from its log
    pub fn needs_replay(&self, chat: &str) -> bool {
        !self.replayed.contains(chat)
    }

    /// Rebuilds a chat's grants from its `grant` events (oldest first): later events win, and a
    /// grant counts only for the time it has left by the wall clock
    pub fn replay(&mut self, chat: &str, records: &[GrantRecord], now: Instant, now_wall: SystemTime) {
        self.replayed.insert(chat.to_string());
        for record in records {
            if record.revoked {
                self.revoke(chat, record.tool.as_deref());
                continue;
            }
            let Some(tool) = &record.tool else { continue };
            let ends = record.at + Duration::from_secs(record.minutes.clamp(1, MAX_GRANT_MINUTES) * 60);
            match ends.duration_since(now_wall) {
                Ok(left) if !left.is_zero() => {
                    self.until.insert((chat.to_string(), tool.clone()), now + left);
                }
                _ => {
                    self.until.remove(&(chat.to_string(), tool.clone()));
                }
            }
        }
    }

    /// Allows `tool` in `chat` for `minutes` (clamped to 1..=MAX_GRANT_MINUTES); returns the minutes
    pub fn grant(&mut self, chat: &str, tool: &str, minutes: Option<u64>, now: Instant) -> u64 {
        let minutes = minutes.unwrap_or(DEFAULT_GRANT_MINUTES).clamp(1, MAX_GRANT_MINUTES);
        self.replayed.insert(chat.to_string());
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
    fn test_replay_restores_what_is_left_and_follows_revokes() {
        let mut grants = Grants::default();
        let (now, wall) = (Instant::now(), SystemTime::now());
        let ago = |minutes: u64| wall - Duration::from_secs(minutes * 60);
        let grant = |tool: &str, minutes: u64, at: SystemTime| GrantRecord { tool: Some(tool.into()), minutes, at, revoked: false };
        let revoke = |tool: Option<&str>, at: SystemTime| GrantRecord { tool: tool.map(str::to_string), minutes: 0, at, revoked: true };
        assert!(grants.needs_replay("c1"));
        grants.replay("c1", &[
            grant("execute_code", 60, ago(20)),   // 40 minutes left
            grant("write_file", 30, ago(45)),     // ran out
            grant("fetch", 60, ago(10)),
            revoke(Some("fetch"), ago(5)),        // revoked later
        ], now, wall);
        assert!(!grants.needs_replay("c1"));
        assert_eq!(grants.of("c1", now), [("execute_code".to_string(), 40)]);
        // A revoke of every tool, then a new grant
        grants.replay("c2", &[grant("a", 60, ago(1)), revoke(None, ago(1)), grant("b", 60, ago(1))], now, wall);
        assert_eq!(grants.of("c2", now), [("b".to_string(), 59)]);
        // A grant made in this run counts as replayed (its event is already in the log)
        grants.grant("c3", "x", None, now);
        assert!(!grants.needs_replay("c3"));
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
