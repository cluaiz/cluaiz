//! llama-server's RAM prompt cache (`--cache-ram`; `tools/server/server-task.cpp`,
//! `server_prompt_cache`): the KV of earlier prompts kept in RAM, so a request that shares a longer
//! start with one of them than with the last prompt restores that state instead of computing the
//! prompt again.
//!
//! Run 40 (`test/cloud/runs/2026-11-22-run40-cluaiz-ollama-website/README.md`, Angle 2): llama-server
//! reused a whole earlier prompt in 94 of 219 requests (Ollama) and 3,019 of 6,350 (llama.cpp); Cluaiz
//! compared only with the last prompt and recomputed about 32 tokens every time, which is where Ollama's
//! lead with 4 requests at once came from.
//!
//! Size: `CLUAIZ_PROMPT_CACHE_MB` (default 0 = off; for example 256 or 1024 turns it on). Off by default
//! (owner, 2026-11-22) because one state is large (103-123 MB for a ~900-token prompt of OLMoE-1B-7B) and the
//! cache would raise the Engine's RAM above the published numbers; llama-server's default is 8192 MiB.
//! The oldest entries go first.

use std::collections::VecDeque;

/// Default limit in MiB when `CLUAIZ_PROMPT_CACHE_MB` is not set: off
const DEFAULT_MB: usize = 0;

/// One earlier prompt: the tokens its KV holds and the sequence state that restores it
pub struct Entry {
    pub tokens: Vec<i32>,
    pub data: Vec<u8>,
}

/// Earlier prompts, oldest first, within `limit` bytes of state
pub struct PromptCache {
    entries: VecDeque<Entry>,
    limit: usize,
    used: usize,
}

/// Tokens two lists share from the start
pub fn common_prefix(a: &[i32], b: &[i32]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

impl PromptCache {
    pub fn new(limit_bytes: usize) -> Self {
        Self { entries: VecDeque::new(), limit: limit_bytes, used: 0 }
    }

    /// The limit from `CLUAIZ_PROMPT_CACHE_MB`, else off
    pub fn from_env() -> Self {
        let mb = std::env::var("CLUAIZ_PROMPT_CACHE_MB")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_MB);
        Self::new(mb.saturating_mul(1024 * 1024))
    }

    pub fn enabled(&self) -> bool {
        self.limit > 0
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry to restore for `prompt` when the context holds `current`, by llama-server's rule
    /// (`server_prompt_cache::load`): it must keep more of itself and cover more of the new prompt than
    /// the current context does, and an entry that would keep less than a quarter of itself is skipped
    /// ("don't trash large prompts")
    pub fn best(&self, prompt: &[i32], current: &[i32]) -> Option<usize> {
        if prompt.is_empty() {
            return None;
        }
        let lcp = common_prefix(current, prompt);
        let mut keep_best = if current.is_empty() { -1.0 } else { lcp as f32 / current.len() as f32 };
        let mut sim_best = lcp as f32 / prompt.len() as f32;
        let mut found = None;
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.tokens.is_empty() {
                continue;
            }
            let lcp = common_prefix(&entry.tokens, prompt);
            let keep = lcp as f32 / entry.tokens.len() as f32;
            let sim = lcp as f32 / prompt.len() as f32;
            if keep < 0.25 {
                continue;
            }
            if keep_best < keep && sim_best < sim {
                keep_best = keep;
                sim_best = sim;
                found = Some(i);
            }
        }
        found
    }

    /// Removes and returns entry `i` (a restored state lives in the context again)
    pub fn take(&mut self, i: usize) -> Option<Entry> {
        let entry = self.entries.remove(i)?;
        self.used -= entry.data.len();
        Some(entry)
    }

    /// Keeps a state; a state larger than the whole limit is not kept, an entry with the same tokens is
    /// replaced, and the oldest go until it fits
    pub fn put(&mut self, tokens: Vec<i32>, data: Vec<u8>) {
        if tokens.is_empty() || data.is_empty() || data.len() > self.limit {
            return;
        }
        if let Some(i) = self.entries.iter().position(|e| e.tokens == tokens) {
            self.take(i);
        }
        while self.used + data.len() > self.limit {
            match self.entries.pop_front() {
                Some(old) => self.used -= old.data.len(),
                None => break,
            }
        }
        self.used += data.len();
        self.entries.push_back(Entry { tokens, data });
    }

    /// Drops every entry (a different model or context layout)
    pub fn clear(&mut self) {
        self.entries.clear();
        self.used = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_earlier_prompt_that_shares_more_wins_over_the_current_one() {
        let mut cache = PromptCache::new(1024);
        cache.put(vec![1, 2, 3, 4, 5, 6], vec![0; 10]);
        // the context holds a prompt that shares only the instruction [1, 2, 3]
        let current = [1, 2, 3, 9, 9, 9];
        assert_eq!(cache.best(&[1, 2, 3, 4, 5, 6, 7], &current), Some(0));
        // a prompt that shares no more with the entry than with the context keeps the context
        assert_eq!(cache.best(&[1, 2, 3, 8], &current), None);
    }

    #[test]
    fn an_entry_that_would_keep_less_than_a_quarter_of_itself_is_skipped() {
        let mut cache = PromptCache::new(1024);
        cache.put((1..=20).collect(), vec![0; 10]);
        // shares 4 of its 20 tokens: f_keep 0.2
        assert_eq!(cache.best(&[1, 2, 3, 4, 99], &[]), None);
        assert_eq!(cache.best(&[1, 2, 3, 4, 5, 99], &[]), Some(0));
    }

    #[test]
    fn the_oldest_go_first_and_the_limit_holds() {
        let mut cache = PromptCache::new(25);
        cache.put(vec![1], vec![0; 10]);
        cache.put(vec![2], vec![0; 10]);
        cache.put(vec![3], vec![0; 10]);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.used, 20);
        assert_eq!(cache.best(&[1], &[]), None, "the first was dropped");
        cache.put(vec![4], vec![0; 30]);
        assert_eq!(cache.len(), 2, "a state larger than the limit is not kept");
        assert!(!PromptCache::new(0).enabled());
    }

    #[test]
    fn the_same_prompt_is_kept_once_and_take_frees_its_bytes() {
        let mut cache = PromptCache::new(100);
        cache.put(vec![1, 2], vec![0; 10]);
        cache.put(vec![1, 2], vec![0; 12]);
        assert_eq!((cache.len(), cache.used), (1, 12));
        let entry = cache.take(0).unwrap();
        assert_eq!((entry.data.len(), cache.used, cache.is_empty()), (12, 0, true));
    }
}
