//! BM25 over a chat's cards, kept in RAM and rebuilt from the cards when needed.

use super::card::Card;
use std::collections::HashMap;

const K1: f32 = 1.2;
const B: f32 = 0.75;

#[derive(Debug, Default)]
pub struct CardIndex {
    postings: HashMap<String, Vec<(usize, u16)>>,
    lengths: Vec<u32>,
    average_length: f32,
}

impl CardIndex {
    pub fn build(cards: &[Card]) -> Self {
        let mut postings: HashMap<String, Vec<(usize, u16)>> = HashMap::new();
        let mut lengths = Vec::with_capacity(cards.len());
        for (index, card) in cards.iter().enumerate() {
            let mut frequency: HashMap<&str, u16> = HashMap::new();
            for term in &card.terms {
                *frequency.entry(term.as_str()).or_default() += 1;
            }
            for (term, tf) in frequency {
                postings.entry(term.to_string()).or_default().push((index, tf));
            }
            lengths.push(card.terms.len() as u32);
        }
        let average_length = if lengths.is_empty() { 0.0 } else { lengths.iter().sum::<u32>() as f32 / lengths.len() as f32 };
        Self { postings, lengths, average_length }
    }

    /// BM25 scores of the cards that share a term with the query: (card index, score)
    pub fn search(&self, query: &[String]) -> Vec<(usize, f32)> {
        let total = self.lengths.len() as f32;
        let mut scores: HashMap<usize, f32> = HashMap::new();
        let mut seen = std::collections::HashSet::new();
        for term in query.iter().filter(|t| seen.insert(t.as_str())) {
            let Some(list) = self.postings.get(term) else { continue };
            let idf = ((total - list.len() as f32 + 0.5) / (list.len() as f32 + 0.5) + 1.0).ln();
            for &(index, tf) in list {
                let length = self.lengths[index] as f32;
                let tf = tf as f32;
                let norm = tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * length / self.average_length.max(1.0)));
                *scores.entry(index).or_default() += idf * norm;
            }
        }
        let mut ranked: Vec<(usize, f32)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked
    }

    pub fn len(&self) -> usize {
        self.lengths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lengths.is_empty()
    }
}
