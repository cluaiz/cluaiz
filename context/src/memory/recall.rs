//! Recall: the earlier messages of a chat worth showing again with the current message
//! (architecture §11.7). Only messages outside the visible prompt are candidates, so a chat that
//! still fits pays nothing.
//!
//! score = bm25 (normalised) + 0.5·shared entities + 0.4·recency + 0.6·importance,
//! restricted to the time window the caller names (see [`super::time`]).

use super::card::{entities, Card};
use super::index::CardIndex;
use crate::analyze::terms;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashSet;

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

pub use super::time::{window, window_in, Window};

/// Earlier messages worth recalling for `message`, best first. `visible` holds the positions
/// already in the prompt; they are never recalled. With a `window`, only messages of that time.
pub fn recall(cards: &[Card], index: &CardIndex, message: &str, visible: &HashSet<u64>, window: Option<Window>, limit: usize) -> Vec<Hit> {
    let query = terms(message);
    let query_entities = entities(message);
    let ranked = index.search(&query);
    // Scores are relative to the best candidate (a visible message, such as the question itself,
    // must not push the hidden ones down)
    let best = ranked
        .iter()
        .find(|(i, _)| !cards[*i].trivial && !visible.contains(&cards[*i].pos))
        .map_or(1.0, |(_, s)| s.max(1.0));
    let newest = cards.iter().map(|c| c.pos).max().unwrap_or(0).max(1) as f32;
    let candidates: Vec<(usize, f32)> = if window.is_some() {
        // Every message of that time is a candidate ("what did we talk about yesterday?" may share
        // no word with them); matching words rank them
        let scores: std::collections::HashMap<usize, f32> = ranked.into_iter().collect();
        (0..cards.len()).map(|i| (i, scores.get(&i).copied().unwrap_or(0.0))).collect()
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
        let hits = recall(&cards, &index, "deploy server ka port kya tha?", &visible, None, 3);
        assert!(!hits.is_empty());
        assert!([0, 1].contains(&cards[hits[0].card].pos), "{hits:?}");
        let hits = recall(&cards, &index, "notes.txt me secret word kya tha?", &visible, None, 3);
        assert_eq!(cards[hits[0].card].pos, 3);
    }

    #[test]
    fn test_nothing_when_everything_is_visible_or_unrelated() {
        let cards = chat();
        let index = CardIndex::build(&cards);
        let all: HashSet<u64> = (0..60).collect();
        assert!(recall(&cards, &index, "deploy server ka port kya tha?", &all, None, 3).is_empty());
        let visible: HashSet<u64> = (40..60).collect();
        assert!(recall(&cards, &index, "tell me a joke", &visible, None, 3).is_empty());
    }

    #[test]
    fn test_a_visible_question_does_not_hide_the_answer() {
        let cards = chat();
        let index = CardIndex::build(&cards);
        let visible: HashSet<u64> = (40..60).collect();
        // Many extra words: the question itself would score far above the old fact
        let hits = recall(&cards, &index, "recall tool use karke dhundo maine deploy server port ke baare me kya kaha tha", &visible, None, 3);
        assert!(hits.iter().any(|h| [0, 1].contains(&cards[h.card].pos)), "{hits:?}");
    }

    #[test]
    fn test_time_windows() {
        let utc = chrono::FixedOffset::east_opt(0).unwrap();
        // The model turned "yesterday" into a date; the question's language does not matter
        let yesterday = window_in(Some("2026-10-23"), Some("2026-10-23"), None, now(), utc).unwrap();
        let cards = chat();
        let index = CardIndex::build(&cards);
        let visible: HashSet<u64> = (40..60).collect();
        for question in ["what did we talk about?", "कल क्या बात हुई थी?", "நேற்று என்ன பேசினோம்?"] {
            let hits = recall(&cards, &index, question, &visible, yesterday, 5);
            assert!(!hits.is_empty(), "{question}");
            assert!(hits.iter().all(|h| cards[h.card].ts.starts_with("2026-10-23")), "{question}: {hits:?}");
        }
        // Without a window the same question recalls nothing by itself
        assert!(recall(&cards, &index, "நேற்று என்ன பேசினோம்?", &visible, None, 5).is_empty());
    }

    #[test]
    fn test_a_tool_run_is_found_by_its_output() {
        let mut messages = vec![
            MessageIn { id: "e0".into(), pos: 0, role: "user".into(), text: "calc.py chalao".into(), ts: "2026-10-24T10:00:00Z".into(), ..Default::default() },
            MessageIn {
                id: "e1".into(),
                pos: 1,
                role: "assistant".into(),
                text: "Done, it ran fine.".into(),
                ts: "2026-10-24T10:00:05Z".into(),
                tools: vec!["code-interpreter".into()],
                tool_text: "stdout: checksum ZEBRA-7781 written".into(),
                ..Default::default()
            },
        ];
        for i in 2..30 {
            messages.push(MessageIn { id: format!("e{i}"), pos: i, role: "user".into(), text: format!("weather talk {i}"), ts: "2026-10-24T11:00:00Z".into(), ..Default::default() });
        }
        let cards = build_cards(&messages);
        let index = CardIndex::build(&cards);
        let visible: HashSet<u64> = (20..30).collect();
        // The checksum is only in the tool's output, not in any message
        let hits = recall(&cards, &index, "ZEBRA-7781 checksum kis run me aaya tha?", &visible, None, 3);
        assert_eq!(cards[hits[0].card].pos, 1, "{hits:?}");
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
