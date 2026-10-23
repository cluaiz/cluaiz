//! The context planner: which segments go into the next prompt, within real token counts.
//!
//! Budget (architecture §10.3):
//!
//! ```text
//! room      = n_ctx − reserve_answer − reserve_thinking − margin
//! available = room − protected
//! ```
//!
//! Then units are admitted in priority order: recent turns, tools in use, evidence (capped),
//! suggestions (capped), older history by relevance, evictable content last. A unit is kept
//! whole or left out whole; nothing is cut. The same input always gives the same plan.

use crate::policy::Policy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a segment is, for the planner
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// Must fit: system prompt, tool protocol, the current message, an unfinished tool step
    Protected,
    /// A past turn's message or tool step; the newest `recent_turns_min` turns are kept first
    History,
    /// A loaded tool called recently
    ToolsInUse,
    /// This turn's recalled evidence
    Evidence,
    /// Tool suggestions
    Suggestions,
    /// Stale blocks, old tool results, idle loaded tools: the first to go
    Evictable,
}

/// One piece of the prompt with its real token count
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub tokens: usize,
    pub class: Class,
    /// Turn it belongs to, counted from 1 (the oldest)
    #[serde(default)]
    pub turn: usize,
    /// Segments with the same group are kept or left out together (a tool call and its result).
    /// History is always grouped by turn.
    #[serde(default)]
    pub group: Option<String>,
    /// Relevance to the current message, 0 to 1; older history and evidence are ranked by it
    #[serde(default)]
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanInput {
    /// Context llama.cpp created
    pub n_ctx: usize,
    /// Tokens kept for the answer
    pub reserve_answer: usize,
    /// Thinking models get the answer reserve again for their thinking
    pub thinking: bool,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub policy: Policy,
}

/// The planner's choice for one segment, with the reason
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub tokens: usize,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub n_ctx: usize,
    pub reserve_answer: usize,
    pub reserve_thinking: usize,
    pub margin: usize,
    pub protected: usize,
    pub available: usize,
    /// Tokens of the kept segments beyond the protected ones
    pub used: usize,
    /// Kept segments, in input order
    pub kept: Vec<Decision>,
    /// Left-out segments, in input order
    pub dropped: Vec<Decision>,
    /// Fewer than `recent_turns_min` recent turns fitted
    pub recent_short: bool,
    /// The plan is at or over the compaction threshold, or left out something that is not
    /// evictable
    pub needs_compaction: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The protected segments alone do not fit; the request fails with the usual clear error
    ProtectedDoesNotFit { needed: usize, room: usize },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::ProtectedDoesNotFit { needed, room } => {
                write!(f, "request ({needed} tokens) exceeds the available context size ({room} tokens)")
            }
        }
    }
}

impl std::error::Error for PlanError {}

/// Admission rank (lower goes first) and the reason given for it
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Recent,
    ToolsInUse,
    Evidence,
    Suggestions,
    Older,
    Evictable,
}

impl Rank {
    fn why(self) -> &'static str {
        match self {
            Rank::Recent => "recent turn",
            Rank::ToolsInUse => "tool in use",
            Rank::Evidence => "evidence",
            Rank::Suggestions => "suggestion",
            Rank::Older => "older turn",
            Rank::Evictable => "evictable",
        }
    }
}

/// Segments admitted or left out together
struct Unit {
    rank: Rank,
    /// Sort keys within the rank: higher score, then newer turn, then id
    score: f32,
    turn: usize,
    key: String,
    members: Vec<usize>,
    tokens: usize,
}

fn percent(value: usize, pct: u8) -> usize {
    value * pct as usize / 100
}

/// Plans the prompt for `input`
pub fn plan(input: &PlanInput) -> Result<Plan, PlanError> {
    let policy = &input.policy;
    let reserve_thinking = if input.thinking { input.reserve_answer } else { 0 };
    let margin = percent(input.n_ctx, policy.margin_pct);
    let room = input.n_ctx.saturating_sub(input.reserve_answer + reserve_thinking + margin);
    let protected: usize = input.segments.iter().filter(|s| s.class == Class::Protected).map(|s| s.tokens).sum();
    if protected > room {
        return Err(PlanError::ProtectedDoesNotFit { needed: protected, room });
    }
    let available = room - protected;

    // The newest `recent_turns_min` turns that have history
    let mut history_turns: Vec<usize> =
        input.segments.iter().filter(|s| s.class == Class::History).map(|s| s.turn).collect();
    history_turns.sort_unstable_by(|a, b| b.cmp(a));
    history_turns.dedup();
    let recent_turns: Vec<usize> = history_turns.iter().take(policy.recent_turns_min).copied().collect();

    // Group into units: history by turn, the rest by group (or alone)
    let mut units: BTreeMap<(Rank, String), Unit> = BTreeMap::new();
    for (index, segment) in input.segments.iter().enumerate() {
        let (rank, key) = match segment.class {
            Class::Protected => continue,
            Class::History if recent_turns.contains(&segment.turn) => (Rank::Recent, format!("turn {}", segment.turn)),
            Class::History => (Rank::Older, format!("turn {}", segment.turn)),
            Class::ToolsInUse => (Rank::ToolsInUse, segment.group.clone().unwrap_or_else(|| segment.id.clone())),
            Class::Evidence => (Rank::Evidence, segment.group.clone().unwrap_or_else(|| segment.id.clone())),
            Class::Suggestions => (Rank::Suggestions, segment.group.clone().unwrap_or_else(|| segment.id.clone())),
            Class::Evictable => (Rank::Evictable, segment.group.clone().unwrap_or_else(|| segment.id.clone())),
        };
        let unit = units.entry((rank, key.clone())).or_insert_with(|| Unit {
            rank,
            score: segment.score,
            turn: segment.turn,
            key,
            members: Vec::new(),
            tokens: 0,
        });
        unit.score = unit.score.max(segment.score);
        unit.turn = unit.turn.max(segment.turn);
        unit.members.push(index);
        unit.tokens += segment.tokens;
    }
    let mut units: Vec<Unit> = units.into_values().collect();
    units.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| {
                // Recent turns go newest first; the other ranks by relevance, then recency
                if a.rank == Rank::Recent {
                    std::cmp::Ordering::Equal
                } else {
                    b.score.total_cmp(&a.score)
                }
            })
            .then_with(|| b.turn.cmp(&a.turn))
            .then_with(|| a.key.cmp(&b.key))
    });

    let caps = |rank: Rank| match rank {
        Rank::Evidence => Some(percent(available, policy.evidence_cap_pct)),
        Rank::Suggestions => Some(percent(available, policy.suggestions_cap_pct)),
        _ => None,
    };
    let mut used = 0usize;
    let mut used_by_rank: BTreeMap<Rank, usize> = BTreeMap::new();
    let mut outcome: Vec<Option<(bool, String)>> = vec![None; input.segments.len()];
    let mut recent_kept = 0usize;
    let mut dropped_needed = false;
    for unit in &units {
        let in_rank = used_by_rank.get(&unit.rank).copied().unwrap_or(0);
        let reason = if let Some(cap) = caps(unit.rank).filter(|cap| in_rank + unit.tokens > *cap) {
            Some(format!("{}: over its cap of {} tokens", unit.rank.why(), cap))
        } else if used + unit.tokens > available {
            Some(format!("{}: no room ({} tokens, {} left)", unit.rank.why(), unit.tokens, available - used))
        } else {
            None
        };
        let kept = reason.is_none();
        if kept {
            used += unit.tokens;
            *used_by_rank.entry(unit.rank).or_insert(0) += unit.tokens;
            if unit.rank == Rank::Recent {
                recent_kept += 1;
            }
        } else if unit.rank != Rank::Evictable {
            dropped_needed = true;
        }
        let why = reason.unwrap_or_else(|| unit.rank.why().to_string());
        for &member in &unit.members {
            outcome[member] = Some((kept, why.clone()));
        }
    }

    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for (segment, outcome) in input.segments.iter().zip(outcome) {
        let (is_kept, why) = outcome.unwrap_or((true, "protected".to_string()));
        let decision = Decision { id: segment.id.clone(), tokens: segment.tokens, why };
        if is_kept {
            kept.push(decision);
        } else {
            dropped.push(decision);
        }
    }
    let recent_short = recent_kept < recent_turns.len();
    let needs_compaction =
        dropped_needed || (available > 0 && used * 100 >= available * policy.compact_at_pct as usize);
    Ok(Plan {
        n_ctx: input.n_ctx,
        reserve_answer: input.reserve_answer,
        reserve_thinking,
        margin,
        protected,
        available,
        used,
        kept,
        dropped,
        recent_short,
        needs_compaction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(id: &str, tokens: usize, class: Class, turn: usize) -> Segment {
        Segment { id: id.into(), tokens, class, turn, group: None, score: 0.0 }
    }

    fn input(n_ctx: usize, segments: Vec<Segment>) -> PlanInput {
        PlanInput { n_ctx, reserve_answer: 256, thinking: false, segments, policy: Policy::default() }
    }

    fn kept_ids(plan: &Plan) -> Vec<&str> {
        plan.kept.iter().map(|d| d.id.as_str()).collect()
    }

    /// Small deterministic generator for the property tests (no extra dependency)
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> usize {
            (self.next() % n) as usize
        }
    }

    fn random_input(rng: &mut Rng) -> PlanInput {
        let classes = [Class::Protected, Class::History, Class::ToolsInUse, Class::Evidence, Class::Suggestions, Class::Evictable];
        let turns = 1 + rng.below(12);
        let mut segments = Vec::new();
        for i in 0..rng.below(40) {
            let class = classes[rng.below(classes.len() as u64)];
            let tokens = 1 + rng.below(if class == Class::Protected { 200 } else { 900 });
            let mut s = segment(&format!("s{i}"), tokens, class, 1 + rng.below(turns as u64));
            s.score = rng.below(100) as f32 / 100.0;
            if rng.below(4) == 0 {
                s.group = Some(format!("g{}", rng.below(5)));
            }
            segments.push(s);
        }
        // The answer reserve is at most a quarter of the context (cluaiz_core::answer_reserve)
        let n_ctx = 1024 + rng.below(8192);
        PlanInput {
            n_ctx,
            reserve_answer: 1 + rng.below(n_ctx as u64 / 4),
            thinking: rng.below(2) == 0,
            segments,
            policy: Policy::default(),
        }
    }

    #[test]
    fn test_never_exceeds_available_and_never_drops_protected() {
        let mut rng = Rng(0x9e3779b97f4a7c15);
        for _ in 0..2000 {
            let input = random_input(&mut rng);
            let Ok(plan) = plan(&input) else { continue };
            assert!(plan.used <= plan.available);
            let kept_total: usize = plan.kept.iter().map(|d| d.tokens).sum();
            assert_eq!(kept_total, plan.protected + plan.used);
            for s in input.segments.iter().filter(|s| s.class == Class::Protected) {
                assert!(plan.kept.iter().any(|d| d.id == s.id), "protected {} dropped", s.id);
            }
            assert!(plan.protected + plan.used + plan.reserve_answer + plan.reserve_thinking + plan.margin <= input.n_ctx);
        }
    }

    #[test]
    fn test_keeps_the_recent_turns_when_they_fit() {
        let mut rng = Rng(0x2545f4914f6cdd1d);
        for _ in 0..2000 {
            let input = random_input(&mut rng);
            let Ok(plan) = plan(&input) else { continue };
            let mut turns: Vec<usize> = input.segments.iter().filter(|s| s.class == Class::History).map(|s| s.turn).collect();
            turns.sort_unstable_by(|a, b| b.cmp(a));
            turns.dedup();
            let recent: Vec<usize> = turns.into_iter().take(input.policy.recent_turns_min).collect();
            let recent_tokens: usize = input
                .segments
                .iter()
                .filter(|s| s.class == Class::History && recent.contains(&s.turn))
                .map(|s| s.tokens)
                .sum();
            if recent_tokens <= plan.available {
                for s in input.segments.iter().filter(|s| s.class == Class::History && recent.contains(&s.turn)) {
                    assert!(plan.kept.iter().any(|d| d.id == s.id), "recent {} dropped", s.id);
                }
                assert!(!plan.recent_short);
            }
        }
    }

    #[test]
    fn test_same_input_same_plan_in_any_order() {
        let mut rng = Rng(0xdeadbeefcafef00d);
        for _ in 0..500 {
            let input = random_input(&mut rng);
            let first = plan(&input);
            assert_eq!(first, plan(&input));
            let mut reversed = input.clone();
            reversed.segments.reverse();
            if let (Ok(a), Ok(b)) = (first, plan(&reversed)) {
                let mut ka = kept_ids(&a);
                let mut kb = kept_ids(&b);
                ka.sort_unstable();
                kb.sort_unstable();
                assert_eq!(ka, kb);
            }
        }
    }

    #[test]
    fn test_protected_that_cannot_fit_is_a_clean_error() {
        let input = input(1000, vec![segment("system", 900, Class::Protected, 0)]);
        assert_eq!(plan(&input), Err(PlanError::ProtectedDoesNotFit { needed: 900, room: 724 }));
        assert!(plan(&input).unwrap_err().to_string().contains("exceeds the available context size"));
    }

    #[test]
    fn test_thinking_doubles_the_reserve() {
        let mut i = input(4096, vec![segment("system", 100, Class::Protected, 0)]);
        i.thinking = true;
        let p = plan(&i).unwrap();
        assert_eq!((p.reserve_answer, p.reserve_thinking, p.margin), (256, 256, 81));
        assert_eq!(p.available, 4096 - 256 - 256 - 81 - 100);
    }

    #[test]
    fn test_older_turns_go_first_and_whole() {
        let segments = vec![
            segment("system", 100, Class::Protected, 0),
            segment("t1.user", 300, Class::History, 1),
            segment("t1.assistant", 300, Class::History, 1),
            segment("t2.user", 100, Class::History, 2),
            segment("t2.assistant", 100, Class::History, 2),
            segment("t3.user", 100, Class::History, 3),
            segment("t3.assistant", 100, Class::History, 3),
            segment("current", 50, Class::Protected, 4),
        ];
        // room 1200 - 256 - 24 = 920; protected 150; available 770: turns 3 and 2 (400), not turn 1 (600)
        let p = plan(&input(1200, segments)).unwrap();
        assert_eq!(kept_ids(&p), ["system", "t2.user", "t2.assistant", "t3.user", "t3.assistant", "current"]);
        assert_eq!(p.dropped.len(), 2);
        assert!(p.dropped[0].why.starts_with("older turn: no room"));
        assert!(p.needs_compaction);
    }

    #[test]
    fn test_caps_and_groups() {
        let mut call = segment("call", 20, Class::ToolsInUse, 3);
        let mut result = segment("result", 500, Class::ToolsInUse, 3);
        call.group = Some("step".into());
        result.group = Some("step".into());
        let segments = vec![
            segment("system", 100, Class::Protected, 0),
            call,
            result,
            segment("evidence", 400, Class::Evidence, 4),
            segment("hint", 30, Class::Suggestions, 4),
        ];
        let p = plan(&input(2000, segments)).unwrap();
        // available 2000 - 256 - 40 - 100 = 1604: evidence cap 240 < 400, hint cap 80 ≥ 30
        assert_eq!(kept_ids(&p), ["system", "call", "result", "hint"]);
        assert!(p.dropped[0].why.contains("over its cap of 240 tokens"));
    }

    #[test]
    fn test_evictable_left_out_does_not_ask_for_compaction() {
        let segments = vec![segment("system", 100, Class::Protected, 0), segment("stale", 5000, Class::Evictable, 1)];
        let p = plan(&input(2000, segments)).unwrap();
        assert_eq!(kept_ids(&p), ["system"]);
        assert!(!p.needs_compaction);
    }
}
