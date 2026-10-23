//! Compaction v1: what to give back when a chat fills its context (architecture §10.4).
//!
//! Cheapest loss first:
//! 1. tool hints (context blocks) of turns outside the newest `recent_turns_min`
//! 2. results of older tool steps, replaced by short placeholders (the newest
//!    `keep_recent_steps` stay whole)
//!
//! Batched: nothing changes unless at least `clear_at_least` tokens are freed, because every
//! compaction re-processes the prompt from its first change. Idle-tool unloads and episode
//! summaries come later; dropping the oldest turns stays in the engine as the last resort.

use serde::{Deserialize, Serialize};

/// One tool step of a turn, in tokens
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepCost {
    pub tokens: usize,
    /// Tokens of the placeholder that would replace the result
    pub placeholder_tokens: usize,
    /// False for results that are already a placeholder or an unload note
    pub clearable: bool,
}

/// One rendered turn, oldest first
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnCost {
    /// Tokens of the hint block in front of the user's message (0 if none)
    pub block_tokens: usize,
    pub steps: Vec<StepCost>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionInput {
    pub turns: Vec<TurnCost>,
    /// Smallest change worth a re-process
    pub clear_at_least: usize,
    /// Tokens that must go for the plan to fit (0 when it only crossed the threshold)
    pub need: usize,
    pub recent_turns_min: usize,
    pub keep_recent_steps: usize,
}

/// What to clear: turn indexes whose block goes, and (turn, step) results that become placeholders
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompactionPlan {
    pub blocks: Vec<usize>,
    pub results: Vec<(usize, usize)>,
    pub freed_tokens: usize,
}

/// The compaction for `input`, or None when it would free less than `clear_at_least`
pub fn compact(input: &CompactionInput) -> Option<CompactionPlan> {
    let target = input.clear_at_least.max(input.need);
    let mut plan = CompactionPlan::default();
    let old_turns = input.turns.len().saturating_sub(input.recent_turns_min);
    for (index, turn) in input.turns.iter().enumerate().take(old_turns) {
        if plan.freed_tokens >= target {
            break;
        }
        if turn.block_tokens > 0 {
            plan.freed_tokens += turn.block_tokens;
            plan.blocks.push(index);
        }
    }
    let steps: Vec<(usize, usize, &StepCost)> = input
        .turns
        .iter()
        .enumerate()
        .flat_map(|(t, turn)| turn.steps.iter().enumerate().map(move |(s, step)| (t, s, step)))
        .collect();
    let clearable = steps.len().saturating_sub(input.keep_recent_steps);
    for &(t, s, step) in &steps[..clearable] {
        if plan.freed_tokens >= target {
            break;
        }
        if step.clearable && step.tokens > step.placeholder_tokens {
            plan.freed_tokens += step.tokens - step.placeholder_tokens;
            plan.results.push((t, s));
        }
    }
    (plan.freed_tokens >= input.clear_at_least).then_some(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(clear_at_least: usize, need: usize) -> CompactionInput {
        let step = |tokens| StepCost { tokens, placeholder_tokens: 9, clearable: true };
        CompactionInput {
            turns: vec![
                TurnCost { block_tokens: 50, steps: vec![step(100), step(101)] },
                TurnCost { block_tokens: 50, steps: vec![step(102), step(103)] },
                TurnCost { block_tokens: 50, steps: vec![step(104)] },
            ],
            clear_at_least,
            need,
            recent_turns_min: 2,
            keep_recent_steps: 3,
        }
    }

    #[test]
    fn test_old_hints_go_first_then_the_oldest_results() {
        let plan = compact(&input(100, 0)).unwrap();
        // Only turn 0 is outside the 2 recent turns; then the oldest result (100 - 9)
        assert_eq!(plan.blocks, [0]);
        assert_eq!(plan.results, [(0, 0)]);
        assert_eq!(plan.freed_tokens, 141);
    }

    #[test]
    fn test_newest_steps_stay_whole() {
        let plan = compact(&input(1, 10_000)).unwrap();
        // 5 steps: only the 2 oldest may be cleared
        assert_eq!(plan.results, [(0, 0), (0, 1)]);
    }

    #[test]
    fn test_batched_or_nothing() {
        assert!(compact(&input(10_000, 0)).is_none());
    }

    #[test]
    fn test_placeholders_and_notes_are_not_cleared_again() {
        let mut i = input(1, 10_000);
        i.turns[0].steps[0].clearable = false;
        assert_eq!(compact(&i).unwrap().results, [(0, 1)]);
    }
}
