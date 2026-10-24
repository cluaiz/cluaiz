//! Recall: the earlier messages of a chat worth showing again with the current message
//! (architecture §11.7). Only messages outside the visible prompt are candidates, so a chat that
//! still fits pays nothing.
//!
//! score = bm25 (normalised) + 0.5·shared entities + 0.4·recency + 0.6·importance,
//! restricted to a time window when the message names one ("kal", "2 din pehle", "last week").

use super::card::{entities, Card};
use super::index::CardIndex;
use crate::analyze::{raw_words, terms};
use chrono::{DateTime, Duration, NaiveTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Smallest score a recalled message needs
pub const MIN_SCORE: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hit {
    /// Index of the card
    pub card: usize,
    pub score: f32,
    /// Why it matched: shared terms and entities
    pub why: Vec<String>,
}

/// A time window named in a message
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

static TIME_WORDS: LazyLock<Value> = LazyLock::new(|| {
    let data: Value = serde_json::from_str(include_str!("../../data/words.json")).unwrap_or_default();
    data.pointer("/memory/time").cloned().unwrap_or_default()
});

fn phrases(key: &str) -> Vec<Vec<String>> {
    TIME_WORDS
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(raw_words).collect())
        .unwrap_or_default()
}

/// The time window a message names, if any ("aaj", "kal", "2 din pehle", "last week", …)
pub fn time_window(text: &str, now: DateTime<Utc>) -> Option<Window> {
    let words = raw_words(text);
    let has = |key: &str| phrases(key).iter().any(|p| super::card::has_phrase(&words, p));
    let day_start = |day: DateTime<Utc>| day.date_naive().and_time(NaiveTime::MIN).and_utc();
    let day = |offset: i64| {
        let start = day_start(now - Duration::days(offset));
        Window { from: start, to: start + Duration::days(1) }
    };
    for phrase in phrases("days_ago") {
        if let Some(at) = words.windows(phrase.len()).position(|w| w == phrase.as_slice()) {
            if let Some(n) = at.checked_sub(1).and_then(|i| words[i].parse::<i64>().ok()) {
                return Some(day(n));
            }
        }
    }
    if has("yesterday") {
        return Some(day(1));
    }
    if has("today") {
        return Some(day(0));
    }
    if has("week") {
        return Some(Window { from: day_start(now - Duration::days(7)), to: now });
    }
    if has("month") {
        return Some(Window { from: day_start(now - Duration::days(31)), to: now });
    }
    None
}

/// Earlier messages worth recalling for `message`, best first. `visible` holds the positions
/// already in the prompt; they are never recalled.
pub fn recall(cards: &[Card], index: &CardIndex, message: &str, visible: &HashSet<u64>, now: DateTime<Utc>, limit: usize) -> Vec<Hit> {
    let query = terms(message);
    let query_entities = entities(message);
    let window = time_window(message, now);
    let ranked = index.search(&query);
    // Scores are relative to the best candidate (a visible message, such as the question itself,
    // must not push the hidden ones down)
    let best = ranked
        .iter()
        .find(|(i, _)| !cards[*i].trivial && !visible.contains(&cards[*i].pos))
        .map_or(1.0, |(_, s)| s.max(1.0));
    let newest = cards.iter().map(|c| c.pos).max().unwrap_or(0).max(1) as f32;
    let candidates: Vec<(usize, f32)> = if window.is_some() && ranked.is_empty() {
        // "kal kya baat hui?": every message of that day is a candidate
        (0..cards.len()).map(|i| (i, 0.0)).collect()
    } else {
        ranked
    };
    let mut hits: Vec<Hit> = candidates
        .into_iter()
        .filter_map(|(i, bm25)| {
            let card = &cards[i];
            if card.trivial || visible.contains(&card.pos) {
                return None;
            }
            if let Some(window) = window {
                let at = DateTime::parse_from_rfc3339(&card.ts).ok()?.with_timezone(&Utc);
                if at < window.from || at >= window.to {
                    return None;
                }
            }
            let card_terms: HashSet<&str> = card.terms.iter().map(String::as_str).collect();
            let mut why: Vec<String> = query.iter().filter(|t| card_terms.contains(t.as_str())).cloned().collect();
            why.dedup();
            let shared_entities: Vec<String> = query_entities
                .files
                .iter()
                .chain(&query_entities.quoted)
                .chain(&query_entities.numbers)
                .filter(|e| {
                    let e = e.to_lowercase();
                    card.entities.files.iter().chain(&card.entities.quoted).chain(&card.entities.numbers).chain(&card.files).any(|c| c.to_lowercase() == e)
                })
                .cloned()
                .collect();
            let recency = card.pos as f32 / newest;
            let score = 3.0 * bm25 / best + 0.5 * shared_entities.len() as f32 + 0.4 * recency + 0.6 * card.importance + if window.is_some() { 1.0 } else { 0.0 };
            // Evidence, not a lone word: two query words, a shared file or value, or a time window
            let evidence = why.len() >= 2 || !shared_entities.is_empty() || window.is_some();
            why.extend(shared_entities);
            (score >= MIN_SCORE && evidence).then_some(Hit { card: i, score, why })
        })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(cards[b.card].pos.cmp(&cards[a.card].pos)));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::card::{build_cards, MessageIn};

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-24T12:00:00Z").unwrap().with_timezone(&Utc)
    }

    fn chat() -> Vec<Card> {
        let msg = |pos: u64, role: &str, text: &str, ts: &str| MessageIn {
            id: format!("ev{pos}"),
            pos,
            role: role.into(),
            text: text.into(),
            ts: ts.into(),
            ..Default::default()
        };
        let mut messages = vec![
            msg(0, "user", "Mera deploy server ka port 8011 hai, yaad rakhna", "2026-10-22T09:00:00Z"),
            msg(1, "assistant", "OK, port 8011 noted for the deploy server.", "2026-10-22T09:00:05Z"),
            msg(2, "user", "notes.txt padho", "2026-10-23T10:00:00Z"),
            msg(3, "assistant", "notes.txt says the secret word is MANGO-42.", "2026-10-23T10:00:09Z"),
            msg(4, "user", "ok", "2026-10-23T10:01:00Z"),
        ];
        for i in 5..60 {
            messages.push(msg(i, if i % 2 == 1 { "user" } else { "assistant" }, &format!("Chatting about cricket score {i} and weather"), "2026-10-24T11:00:00Z"));
        }
        build_cards(&messages)
    }

    #[test]
    fn test_recall_finds_an_old_fact_outside_the_window() {
        let cards = chat();
        let index = CardIndex::build(&cards);
        let visible: HashSet<u64> = (40..60).collect();
        let hits = recall(&cards, &index, "deploy server ka port kya tha?", &visible, now(), 3);
        assert!(!hits.is_empty());
        assert!([0, 1].contains(&cards[hits[0].card].pos), "{hits:?}");
        let hits = recall(&cards, &index, "notes.txt me secret word kya tha?", &visible, now(), 3);
        assert_eq!(cards[hits[0].card].pos, 3);
    }

    #[test]
    fn test_nothing_when_everything_is_visible_or_unrelated() {
        let cards = chat();
        let index = CardIndex::build(&cards);
        let all: HashSet<u64> = (0..60).collect();
        assert!(recall(&cards, &index, "deploy server ka port kya tha?", &all, now(), 3).is_empty());
        let visible: HashSet<u64> = (40..60).collect();
        assert!(recall(&cards, &index, "tell me a joke", &visible, now(), 3).is_empty());
    }

    #[test]
    fn test_a_visible_question_does_not_hide_the_answer() {
        let cards = chat();
        let index = CardIndex::build(&cards);
        let visible: HashSet<u64> = (40..60).collect();
        // Many extra words: the question itself would score far above the old fact
        let hits = recall(&cards, &index, "recall tool use karke dhundo maine deploy server port ke baare me kya kaha tha", &visible, now(), 3);
        assert!(hits.iter().any(|h| [0, 1].contains(&cards[h.card].pos)), "{hits:?}");
    }

    #[test]
    fn test_time_windows() {
        let w = time_window("kal kya baat hui?", now()).unwrap();
        assert_eq!(w.from.to_rfc3339(), "2026-10-23T00:00:00+00:00");
        assert!(time_window("2 din pehle maine kya kaha", now()).unwrap().from.to_rfc3339().starts_with("2026-10-22"));
        assert!(time_window("what is rust", now()).is_none());
        let cards = chat();
        let index = CardIndex::build(&cards);
        let visible: HashSet<u64> = (40..60).collect();
        let hits = recall(&cards, &index, "kal kya baat hui thi?", &visible, now(), 5);
        assert!(hits.iter().all(|h| cards[h.card].ts.starts_with("2026-10-23")), "{hits:?}");
        assert!(!hits.is_empty());
    }

    #[test]
    fn test_index_rebuild_is_fast() {
        let messages: Vec<MessageIn> = (0..20_000)
            .map(|i| MessageIn { id: format!("e{i}"), pos: i, role: "user".into(), text: format!("topic {} file src/m{}.rs value {}", i % 101, i % 37, i), ts: "2026-10-24T11:00:00Z".into(), ..Default::default() })
            .collect();
        let cards = build_cards(&messages);
        let started = std::time::Instant::now();
        let index = CardIndex::build(&cards);
        let elapsed = started.elapsed();
        eprintln!("20k-card index built in {elapsed:?}");
        assert_eq!(index.len(), 20_000);
        assert!(elapsed.as_millis() <= 150, "took {elapsed:?}");
    }
}
