//! The Chat Map graph (architecture §11.5), built by rules from cards and episodes: messages
//! follow each other, belong to an episode, use tools, touch files and mention values. Recall uses
//! it to bring along a message linked to a hit (the same file or value, a later message on the same
//! thing, which may correct it).

use super::card::Card;
use super::episode::Episode;
use super::recall::Hit;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Node {
    /// `m:<pos>`, `ep:<pos>`, `tool:<name>`, `file:<path>`, `value:<text>`
    pub id: String,
    pub kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// next, part_of, uses_tool, touches_file, mentions, updates
    pub kind: &'static str,
}

#[derive(Debug, Default, Serialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// What links a message to others: files, tools and named values
fn anchors(card: &Card) -> Vec<String> {
    let mut out: Vec<String> = card.files.iter().chain(&card.entities.files).map(|f| format!("file:{}", f.to_lowercase())).collect();
    out.extend(card.tools.iter().map(|t| format!("tool:{t}")));
    out.extend(card.entities.numbers.iter().chain(&card.entities.quoted).map(|v| format!("value:{}", v.to_lowercase())));
    out.sort();
    out.dedup();
    out
}

/// Keywords two messages share before the later one counts as saying more about the same thing
const UPDATE_SHARED_KEYWORDS: usize = 2;

/// Whether `later` says more about what `earlier` said (same speaker, two shared keywords): a
/// correction ("meeting Wednesday, not Monday") is one, in any language
fn updates(later: &Card, earlier: &Card) -> bool {
    later.role == earlier.role && later.keywords.iter().filter(|k| earlier.keywords.contains(k)).count() >= UPDATE_SHARED_KEYWORDS
}

/// The graph of a chat
pub fn build_graph(cards: &[Card], episodes: &[Episode]) -> Graph {
    let mut graph = Graph::default();
    let mut seen = HashSet::new();
    let mut node = |graph: &mut Graph, id: String, kind: &'static str| {
        if seen.insert(id.clone()) {
            graph.nodes.push(Node { id, kind });
        }
    };
    for episode in episodes {
        node(&mut graph, episode.id.clone(), "episode");
    }
    let mut previous: Option<String> = None;
    for card in cards {
        let id = format!("m:{}", card.pos);
        node(&mut graph, id.clone(), "message");
        if let Some(prev) = previous.replace(id.clone()) {
            graph.edges.push(Edge { from: prev, to: id.clone(), kind: "next" });
        }
        if let Some(episode) = episodes.iter().find(|e| (e.first_pos..=e.last_pos).contains(&card.pos)) {
            graph.edges.push(Edge { from: id.clone(), to: episode.id.clone(), kind: "part_of" });
        }
        for anchor in anchors(card) {
            let kind = if anchor.starts_with("file:") {
                "touches_file"
            } else if anchor.starts_with("tool:") {
                "uses_tool"
            } else {
                "mentions"
            };
            let node_kind = anchor.split(':').next().unwrap_or("value");
            let node_kind: &'static str = match node_kind {
                "file" => "file",
                "tool" => "tool",
                _ => "value",
            };
            node(&mut graph, anchor.clone(), node_kind);
            graph.edges.push(Edge { from: id.clone(), to: anchor, kind });
        }
    }
    // A user message points at the latest earlier one it says more about
    for (i, card) in cards.iter().enumerate().filter(|(_, c)| c.role == "user" && !c.trivial) {
        if let Some(earlier) = cards[..i].iter().rev().take(200).find(|earlier| updates(card, earlier)) {
            graph.edges.push(Edge { from: format!("m:{}", card.pos), to: format!("m:{}", earlier.pos), kind: "updates" });
        }
    }
    graph
}

/// Hits plus linked messages (one hop): a later correction of a hit, and messages that share a
/// file or value with it. Linked messages get half the hit's score; `visible` ones are skipped.
pub fn expand(hits: &[Hit], cards: &[Card], visible: &HashSet<u64>, limit: usize) -> Vec<Hit> {
    let mut by_anchor: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, card) in cards.iter().enumerate() {
        for anchor in anchors(card).into_iter().filter(|a| !a.starts_with("tool:")) {
            by_anchor.entry(anchor).or_default().push(i);
        }
    }
    let mut out: Vec<Hit> = hits.to_vec();
    let mut taken: HashSet<usize> = hits.iter().map(|h| h.card).collect();
    for hit in hits {
        let card = &cards[hit.card];
        let mut linked: Vec<(usize, String)> = Vec::new();
        // The latest later message on the same thing (it may correct the hit)
        if let Some(i) = (hit.card + 1..cards.len()).rev().find(|&i| updates(&cards[i], card)) {
            linked.push((i, "later on the same".to_string()));
        }
        for anchor in anchors(card).into_iter().filter(|a| !a.starts_with("tool:")) {
            for &i in by_anchor.get(&anchor).into_iter().flatten() {
                linked.push((i, format!("same {anchor}")));
            }
        }
        for (i, why) in linked {
            if out.len() >= limit {
                break;
            }
            if taken.contains(&i) || visible.contains(&cards[i].pos) || cards[i].trivial {
                continue;
            }
            taken.insert(i);
            out.push(Hit { card: i, score: hit.score * 0.5, why: vec![why] });
        }
    }
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::card::{build_cards, MessageIn};
    use crate::memory::episode::{episodes, EpisodeRules};

    fn msg(pos: u64, role: &str, text: &str) -> MessageIn {
        MessageIn { id: format!("e{pos}"), pos, role: role.into(), text: text.into(), ts: "2026-10-24T09:00:00Z".into(), ..Default::default() }
    }

    #[test]
    fn test_edges_and_expansion_bring_the_later_message() {
        let messages = vec![
            msg(0, "user", "Team meeting Monday ko hai."),
            msg(1, "assistant", "Noted."),
            msg(2, "user", "cricket score batao"),
            msg(3, "assistant", "250/3"),
            msg(4, "user", "Nahi, galat bola: team meeting Wednesday ko hai."),
            msg(5, "assistant", "Noted, Wednesday."),
        ];
        let cards = build_cards(&messages);
        let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
        let eps = episodes(&cards, &texts, EpisodeRules::default());
        let graph = build_graph(&cards, &eps);
        assert!(graph.edges.iter().any(|e| e.kind == "updates" && e.from == "m:4" && e.to == "m:0"), "{:?}", graph.edges);
        assert!(graph.edges.iter().any(|e| e.kind == "next"));
        // Recall found only the old statement: expansion brings the later one, which corrects it
        let hits = vec![Hit { card: 0, score: 3.0, why: vec!["meeting".into()] }];
        let expanded = expand(&hits, &cards, &HashSet::new(), 3);
        assert!(expanded.iter().any(|h| cards[h.card].pos == 4), "{expanded:?}");
    }
}
