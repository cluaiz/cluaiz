//! Cards: one index record per message, built by rules (architecture §11.2). A card never holds
//! the message text; the text stays in the chat's event log and is read by reference.

use crate::analyze::{raw_words, terms};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// A message as the card builder sees it
#[derive(Debug, Clone, Default)]
pub struct MessageIn {
    /// Event id of the message (the reference recall gives back)
    pub id: String,
    /// Position in the transcript
    pub pos: u64,
    pub role: String,
    pub text: String,
    /// RFC 3339
    pub ts: String,
    /// Tools the Engine ran while answering this message (assistant messages)
    pub tools: Vec<String>,
    /// Files those tools touched
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Entities {
    pub files: Vec<String>,
    pub urls: Vec<String>,
    pub numbers: Vec<String>,
    pub quoted: Vec<String>,
}

impl Entities {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.urls.is_empty() && self.numbers.is_empty() && self.quoted.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub pos: u64,
    pub role: String,
    pub ts: String,
    /// Estimated tokens of the message (characters ÷ 3.5)
    pub tokens: usize,
    /// Short acknowledgement with nothing new ("ok", "haan", "theek hai")
    pub trivial: bool,
    /// Top terms by TF-IDF against the chat's own frequencies
    pub keywords: Vec<String>,
    pub entities: Entities,
    pub tools: Vec<String>,
    pub files: Vec<String>,
    /// correction, decision, question, request, error
    pub signals: Vec<String>,
    pub importance: f32,
    /// Every search term of the message (for the index; not saved with the card)
    #[serde(skip)]
    pub terms: Vec<String>,
}

const MAX_KEYWORDS: usize = 8;

struct SignalWords {
    acknowledgement: HashSet<String>,
    lists: Vec<(&'static str, Vec<Vec<String>>)>,
}

static SIGNALS: LazyLock<SignalWords> = LazyLock::new(|| {
    let data: Value = serde_json::from_str(include_str!("../../data/words.json")).unwrap_or_default();
    let memory = data.get("memory").cloned().unwrap_or_default();
    let list = |key: &str| -> Vec<String> {
        memory.get(key).and_then(Value::as_array).map(|items| items.iter().filter_map(Value::as_str).map(str::to_lowercase).collect()).unwrap_or_default()
    };
    let phrases = |key: &str| -> Vec<Vec<String>> { list(key).iter().map(|p| raw_words(p)).filter(|p| !p.is_empty()).collect() };
    SignalWords {
        acknowledgement: list("acknowledgement").into_iter().collect(),
        lists: vec![("correction", phrases("correction")), ("decision", phrases("decision")), ("error", phrases("error"))],
    }
});

/// Whether the words contain `phrase` as consecutive whole words
pub fn has_phrase(words: &[String], phrase: &[String]) -> bool {
    !phrase.is_empty() && words.windows(phrase.len()).any(|window| window == phrase)
}

/// Files, links, numbers and quoted strings named in a text
pub fn entities(text: &str) -> Entities {
    let mut found = Entities::default();
    for token in text.split_whitespace() {
        let token = token.trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | ',' | ';' | ')' | '(' | '[' | ']' | '?' | '!' | ':' | '*'));
        if token.contains("://") || token.starts_with("www.") {
            found.urls.push(token.trim_end_matches('.').to_string());
        } else if is_file(token) {
            found.files.push(token.trim_end_matches('.').to_string());
        } else if token.chars().any(|c| c.is_ascii_digit()) && token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '%')) {
            found.numbers.push(token.trim_end_matches('.').to_string());
        }
    }
    // Quoted strings ("…" or `…`), short ones only
    for quote in ['"', '`'] {
        let parts: Vec<&str> = text.split(quote).collect();
        for inner in parts.iter().skip(1).step_by(2) {
            let inner = inner.trim();
            if !inner.is_empty() && inner.len() <= 60 && !inner.contains('\n') {
                found.quoted.push(inner.to_string());
            }
        }
    }
    for list in [&mut found.files, &mut found.urls, &mut found.numbers, &mut found.quoted] {
        let mut seen = HashSet::new();
        list.retain(|item| seen.insert(item.to_lowercase()));
    }
    found
}

fn is_file(token: &str) -> bool {
    let token = token.trim_end_matches('.');
    token.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()) && !ext.chars().all(|c| c.is_ascii_digit())
    }) || ((token.contains('/') || token.contains('\\')) && token.len() > 2)
}

/// Cards for a chat's messages; keywords are ranked against the chat's own term frequencies
pub fn build_cards(messages: &[MessageIn]) -> Vec<Card> {
    let message_terms: Vec<Vec<String>> = messages.iter().map(|m| terms(&m.text)).collect();
    let mut document_frequency: HashMap<&str, usize> = HashMap::new();
    for terms in &message_terms {
        for term in terms.iter().collect::<HashSet<_>>() {
            *document_frequency.entry(term.as_str()).or_default() += 1;
        }
    }
    let total = messages.len().max(1) as f32;
    messages
        .iter()
        .zip(&message_terms)
        .map(|(message, message_terms)| {
            let mut frequency: HashMap<&str, usize> = HashMap::new();
            for term in message_terms {
                *frequency.entry(term.as_str()).or_default() += 1;
            }
            let mut ranked: Vec<(&str, f32)> = frequency
                .iter()
                .map(|(term, tf)| (*term, *tf as f32 * (1.0 + total / *document_frequency.get(term).unwrap_or(&1) as f32).ln()))
                .collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
            let keywords: Vec<String> = ranked.iter().take(MAX_KEYWORDS).map(|(t, _)| t.to_string()).collect();

            let entities = entities(&message.text);
            let words = raw_words(&message.text);
            let signals_data = &*SIGNALS;
            let mut signals: Vec<String> = signals_data
                .lists
                .iter()
                .filter(|(_, phrases)| phrases.iter().any(|p| has_phrase(&words, p)))
                .map(|(name, _)| name.to_string())
                .collect();
            let is_user = message.role.eq_ignore_ascii_case("user");
            if message.text.trim_end().ends_with('?') {
                signals.push("question".into());
            }
            if is_user && message_terms.iter().any(|t| crate::analyze::WORDS.actions.contains(t)) {
                signals.push("request".into());
            }
            // Corrections and decisions count when the user says them
            if !is_user {
                signals.retain(|s| s != "correction" && s != "decision");
            }
            let trivial = entities.is_empty()
                && message.tools.is_empty()
                && words.len() <= 4
                && words.iter().all(|w| signals_data.acknowledgement.contains(w));
            let mut importance: f32 = if trivial { 0.0 } else { 0.2 };
            if !trivial {
                for (signal, weight) in [("decision", 0.3), ("correction", 0.3), ("error", 0.1), ("request", 0.1)] {
                    if signals.iter().any(|s| s == signal) {
                        importance += weight;
                    }
                }
                if !entities.is_empty() {
                    importance += 0.1;
                }
                if !message.tools.is_empty() {
                    importance += 0.1;
                }
            }
            let mut index_terms = message_terms.clone();
            index_terms.extend(entities.files.iter().flat_map(|f| terms(f)));
            index_terms.extend(message.tools.iter().flat_map(|t| terms(t)));
            index_terms.extend(message.files.iter().flat_map(|f| terms(f)));
            Card {
                id: message.id.clone(),
                pos: message.pos,
                role: message.role.to_lowercase(),
                ts: message.ts.clone(),
                tokens: (message.text.len() as f32 / 3.5).ceil() as usize,
                trivial,
                keywords,
                entities,
                tools: message.tools.clone(),
                files: message.files.clone(),
                signals,
                importance: importance.min(1.0),
                terms: index_terms,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(pos: u64, role: &str, text: &str) -> MessageIn {
        MessageIn { id: format!("ev{pos}"), pos, role: role.into(), text: text.into(), ts: "2026-10-24T10:00:00Z".into(), ..Default::default() }
    }

    #[test]
    fn test_cards_for_every_message() {
        let cards = build_cards(&[
            msg(0, "user", "notes.txt padho aur secret word batao"),
            msg(1, "assistant", "The secret word is \"MANGO-42\"."),
            msg(2, "user", "ok"),
            msg(3, "user", "nahi, galat file thi, README.md padho"),
            msg(4, "user", "final: isi ko rakho, we use port 8011"),
        ]);
        assert_eq!(cards.len(), 5);
        assert_eq!(cards[0].entities.files, ["notes.txt"]);
        assert!(cards[0].signals.contains(&"request".to_string()));
        assert!(cards[1].entities.quoted.contains(&"MANGO-42".to_string()));
        assert!(cards[2].trivial && cards[2].importance == 0.0);
        assert!(cards[3].signals.contains(&"correction".to_string()));
        assert!(cards[4].signals.contains(&"decision".to_string()));
        assert!(cards[4].entities.numbers.contains(&"8011".to_string()));
        assert!(cards[4].importance > cards[0].importance);
        assert!(cards[0].keywords.iter().any(|k| k == "secret"));
    }

    #[test]
    fn test_twenty_thousand_cards_build_fast() {
        let messages: Vec<MessageIn> = (0..20_000)
            .map(|i| msg(i, if i % 2 == 0 { "user" } else { "assistant" }, &format!("Message {i} about crate {} and file src/m{}.rs with notes", i % 97, i % 31)))
            .collect();
        let started = std::time::Instant::now();
        let cards = build_cards(&messages);
        let elapsed = started.elapsed();
        eprintln!("20k cards built in {elapsed:?}");
        assert_eq!(cards.len(), 20_000);
        assert!(elapsed.as_secs_f64() < 2.0, "took {elapsed:?}");
    }
}
