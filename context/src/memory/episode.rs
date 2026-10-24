//! Episodes: stretches of a chat about one thing (architecture §11.3), found by rules.
//!
//! A new episode starts at a user message when more than `gap_minutes` passed since the last
//! message, when the user says so ("new topic", "naya topic", "/new"), or when the message
//! shares almost no keywords with the current episode (and is not a short follow-up).

use super::card::{has_phrase, Card};
use crate::analyze::{raw_words, WORDS};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    /// `ep:<position of its first message>`
    pub id: String,
    pub first_pos: u64,
    pub last_pos: u64,
    pub start: String,
    pub end: String,
    pub messages: usize,
    /// Most frequent keywords of its messages
    pub keywords: Vec<String>,
    /// A rule title: its top keywords
    pub title: String,
    pub files: Vec<String>,
    pub tools: Vec<String>,
    /// Positions of messages marked as corrections or decisions
    pub key_moments: Vec<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct EpisodeRules {
    pub gap_minutes: i64,
    /// Below this keyword overlap with the episode, a new episode starts
    pub min_overlap: f32,
    /// An episode is not split before it has this many messages
    pub min_messages: usize,
}

impl Default for EpisodeRules {
    fn default() -> Self {
        Self { gap_minutes: 30, min_overlap: 0.08, min_messages: 4 }
    }
}

static NEW_TOPIC: LazyLock<Vec<Vec<String>>> = LazyLock::new(|| {
    let data: Value = serde_json::from_str(include_str!("../../data/words.json")).unwrap_or_default();
    data.pointer("/memory/new_topic")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(raw_words).collect())
        .unwrap_or_default()
});

const EPISODE_KEYWORDS: usize = 6;

fn minutes_between(a: &str, b: &str) -> Option<i64> {
    let a = DateTime::parse_from_rfc3339(a).ok()?;
    let b = DateTime::parse_from_rfc3339(b).ok()?;
    Some((b - a).num_minutes())
}

/// Episodes of a chat, oldest first. `texts` are the messages' texts (same order as `cards`),
/// used only for the "new topic" words.
pub fn episodes(cards: &[Card], texts: &[&str], rules: EpisodeRules) -> Vec<Episode> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut topic: HashSet<&str> = HashSet::new();
    for (i, card) in cards.iter().enumerate() {
        let starts_new = match groups.last() {
            None => true,
            Some(current) if card.role == "user" => {
                let words = raw_words(texts.get(i).copied().unwrap_or_default());
                let said_so = NEW_TOPIC.iter().any(|p| has_phrase(&words, p));
                let gap = cards[current[current.len() - 1]].ts.as_str();
                let long_pause = minutes_between(gap, &card.ts).is_some_and(|m| m > rules.gap_minutes);
                let follow_up = words.len() <= 6 && words.iter().any(|w| WORDS.followup.contains(w) || WORDS.repeat.contains(w));
                let keywords: HashSet<&str> = card.keywords.iter().map(String::as_str).collect();
                let shared = keywords.intersection(&topic).count() as f32;
                let union = keywords.union(&topic).count().max(1) as f32;
                let drifted = !card.trivial && !follow_up && !keywords.is_empty() && shared / union < rules.min_overlap && current.len() >= rules.min_messages;
                said_so || long_pause || drifted
            }
            Some(_) => false,
        };
        if starts_new {
            groups.push(Vec::new());
            topic.clear();
        }
        groups.last_mut().expect("a group exists").push(i);
        topic.extend(card.keywords.iter().map(String::as_str));
    }
    groups
        .into_iter()
        .map(|group| {
            let first = &cards[group[0]];
            let last = &cards[group[group.len() - 1]];
            let mut frequency: HashMap<&str, usize> = HashMap::new();
            for &i in &group {
                for keyword in &cards[i].keywords {
                    *frequency.entry(keyword.as_str()).or_default() += 1;
                }
            }
            let mut ranked: Vec<(&str, usize)> = frequency.into_iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let keywords: Vec<String> = ranked.iter().take(EPISODE_KEYWORDS).map(|(k, _)| k.to_string()).collect();
            let mut files: Vec<String> = group.iter().flat_map(|&i| cards[i].files.iter().chain(&cards[i].entities.files).cloned()).collect();
            files.sort();
            files.dedup();
            let mut tools: Vec<String> = group.iter().flat_map(|&i| cards[i].tools.clone()).collect();
            tools.sort();
            tools.dedup();
            let key_moments = group
                .iter()
                .filter(|&&i| cards[i].signals.iter().any(|s| s == "correction" || s == "decision"))
                .map(|&i| cards[i].pos)
                .collect();
            Episode {
                id: format!("ep:{}", first.pos),
                first_pos: first.pos,
                last_pos: last.pos,
                start: first.ts.clone(),
                end: last.ts.clone(),
                messages: group.len(),
                title: keywords.iter().take(4).cloned().collect::<Vec<_>>().join(", "),
                keywords,
                files,
                tools,
                key_moments,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::card::{build_cards, MessageIn};

    fn msg(pos: u64, role: &str, text: &str, ts: &str) -> MessageIn {
        MessageIn { id: format!("e{pos}"), pos, role: role.into(), text: text.into(), ts: ts.into(), ..Default::default() }
    }

    #[test]
    fn test_breaks_on_pause_words_and_drift() {
        let messages = vec![
            msg(0, "user", "deploy server ka port 8011 set karo", "2026-10-24T09:00:00Z"),
            msg(1, "assistant", "deploy server port 8011 set ho gaya", "2026-10-24T09:00:05Z"),
            msg(2, "user", "deploy server restart karo", "2026-10-24T09:01:00Z"),
            msg(3, "assistant", "deploy server restarted", "2026-10-24T09:01:05Z"),
            // An hour later
            msg(4, "user", "biryani recipe batao", "2026-10-24T10:30:00Z"),
            msg(5, "assistant", "biryani: rice, masala, dum", "2026-10-24T10:30:05Z"),
            msg(6, "user", "aur raita?", "2026-10-24T10:31:00Z"),
            msg(7, "assistant", "raita: dahi, kheera", "2026-10-24T10:31:05Z"),
            // The user says so
            msg(8, "user", "naya topic: cricket score kya hai", "2026-10-24T10:32:00Z"),
            msg(9, "assistant", "cricket score 250/3", "2026-10-24T10:32:05Z"),
        ];
        let cards = build_cards(&messages);
        let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
        let found = episodes(&cards, &texts, EpisodeRules::default());
        let spans: Vec<(u64, u64)> = found.iter().map(|e| (e.first_pos, e.last_pos)).collect();
        assert_eq!(spans, [(0, 3), (4, 7), (8, 9)]);
        assert!(found[0].keywords.iter().any(|k| k == "deploy" || k == "server"), "{:?}", found[0].keywords);
    }

    #[test]
    fn test_follow_ups_stay_in_the_episode() {
        let messages = vec![
            msg(0, "user", "notes.txt padho", "2026-10-24T09:00:00Z"),
            msg(1, "assistant", "notes.txt: secret MANGO-42", "2026-10-24T09:00:05Z"),
            msg(2, "user", "phir se padho", "2026-10-24T09:01:00Z"),
            msg(3, "assistant", "notes.txt: secret MANGO-42", "2026-10-24T09:01:05Z"),
            msg(4, "user", "isko dobara chalao", "2026-10-24T09:02:00Z"),
        ];
        let cards = build_cards(&messages);
        let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(episodes(&cards, &texts, EpisodeRules::default()).len(), 1);
    }
}
