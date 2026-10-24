//! Tasks (architecture §11.4): work the user and the model are doing over several turns.
//!
//! Implicit tasks come from episodes in which tools changed something (write, edit, create, move,
//! delete, run, execute). The model can also keep a task itself with `task_update` (goal, stage,
//! checkpoint), which wins over the implicit one. The active task's checkpoint travels with the
//! current message when earlier turns are not all shown, so the model does not lose its place.

use super::card::Card;
use super::episode::Episode;
use serde::{Deserialize, Serialize};

/// Tool names (or parts of them) that change something
const CHANGING: [&str; 8] = ["write", "edit", "create", "move", "delete", "remove", "execute", "run"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    /// open, in_progress, blocked, done, abandoned
    pub stage: String,
    pub from_pos: u64,
    pub to_pos: u64,
    pub files: Vec<String>,
    pub tools: Vec<String>,
    /// Where the work stands (at most ~100 tokens)
    pub checkpoint: String,
    /// "rules" or "model"
    pub source: String,
}

/// A `task_update` the model made
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskUpdate {
    pub goal: String,
    pub stage: String,
    #[serde(default)]
    pub checkpoint: String,
    /// Position of the turn it was made in
    pub pos: u64,
}

fn changes_something(tool: &str) -> bool {
    let tool = tool.to_lowercase();
    CHANGING.iter().any(|word| tool.contains(word))
}

fn clip(text: &str, words: usize) -> String {
    let all: Vec<&str> = text.split_whitespace().collect();
    if all.len() <= words {
        all.join(" ")
    } else {
        format!("{}…", all[..words].join(" "))
    }
}

/// Tasks of a chat, oldest first: implicit ones from episodes, then the model's own (the newest
/// update of a goal wins). `user_texts` gives the text of a user message by position.
pub fn tasks(cards: &[Card], episodes: &[Episode], updates: &[TaskUpdate], user_text: &dyn Fn(u64) -> Option<String>) -> Vec<Task> {
    let last_episode = episodes.last().map(|e| e.id.clone());
    let mut out: Vec<Task> = episodes
        .iter()
        .filter(|e| e.tools.iter().any(|t| changes_something(t)))
        .map(|e| {
            let runs: Vec<&Card> = cards.iter().filter(|c| (e.first_pos..=e.last_pos).contains(&c.pos) && !c.tools.is_empty()).collect();
            let done: Vec<String> = runs
                .iter()
                .map(|c| if c.files.is_empty() { c.tools.join(", ") } else { format!("{} on {}", c.tools.join(", "), c.files.join(", ")) })
                .collect();
            let first_request = user_text(e.first_pos).map(|t| clip(&t, 30)).unwrap_or_default();
            let last_request = cards
                .iter()
                .rev()
                .find(|c| c.role == "user" && (e.first_pos..=e.last_pos).contains(&c.pos))
                .and_then(|c| user_text(c.pos))
                .map(|t| clip(&t, 25))
                .unwrap_or_default();
            let mut checkpoint = format!("Started with: \"{first_request}\". Done so far: {}.", done.join("; "));
            if !last_request.is_empty() && last_request != first_request {
                checkpoint.push_str(&format!(" Last request: \"{last_request}\"."));
            }
            Task {
                id: format!("task:{}", e.first_pos),
                title: e.title.clone(),
                stage: if Some(&e.id) == last_episode.as_ref() { "in_progress".into() } else { "done".into() },
                from_pos: e.first_pos,
                to_pos: e.last_pos,
                files: e.files.clone(),
                tools: e.tools.clone(),
                checkpoint,
                source: "rules".into(),
            }
        })
        .collect();
    // The model's own tasks: one per goal, its newest update
    let mut goals: Vec<&TaskUpdate> = Vec::new();
    for update in updates {
        match goals.iter_mut().find(|g| g.goal.eq_ignore_ascii_case(&update.goal)) {
            Some(slot) => *slot = update,
            None => goals.push(update),
        }
    }
    for update in goals {
        let first = updates.iter().find(|u| u.goal.eq_ignore_ascii_case(&update.goal)).map_or(update.pos, |u| u.pos);
        out.push(Task {
            id: format!("task:m{first}"),
            title: clip(&update.goal, 12),
            stage: update.stage.clone(),
            from_pos: first,
            to_pos: update.pos,
            files: Vec::new(),
            tools: Vec::new(),
            checkpoint: clip(&update.checkpoint, 80),
            source: "model".into(),
        });
    }
    out
}

/// The task to keep in front of the model: the model's own open one if any, else the implicit
/// task of the current episode
pub fn active(tasks: &[Task]) -> Option<&Task> {
    let open = |t: &&Task| matches!(t.stage.as_str(), "open" | "in_progress" | "blocked");
    let latest_model = tasks.iter().filter(|t| t.source == "model").max_by_key(|t| t.to_pos);
    let implicit = tasks.iter().filter(|t| t.source == "rules").filter(open).max_by_key(|t| t.to_pos);
    match (latest_model, implicit) {
        // The model's own word on the current work wins, also when it says the work is done
        (Some(model), Some(rules)) if model.to_pos >= rules.from_pos => Some(model).filter(open),
        (Some(model), Some(rules)) => Some(model).filter(open).or(Some(rules)),
        (Some(model), None) => Some(model).filter(open),
        (None, rules) => rules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::card::{build_cards, MessageIn};
    use crate::memory::episode::{episodes, EpisodeRules};

    fn msg(pos: u64, role: &str, text: &str, tools: &[&str], files: &[&str]) -> MessageIn {
        MessageIn {
            id: format!("e{pos}"),
            pos,
            role: role.into(),
            text: text.into(),
            ts: "2026-10-24T09:00:00Z".into(),
            tools: tools.iter().map(|s| s.to_string()).collect(),
            files: files.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn test_implicit_task_from_changes_and_model_updates_win() {
        let messages = vec![
            msg(0, "user", "Teen files banao a.txt b.txt c.txt, ek ek karke; har file me uska naam likho", &[], &[]),
            msg(1, "assistant", "a.txt bana di", &["write_file"], &["a.txt"]),
            msg(2, "user", "next", &[], &[]),
            msg(3, "assistant", "b.txt bana di", &["write_file"], &["b.txt"]),
        ];
        let cards = build_cards(&messages);
        let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
        let eps = episodes(&cards, &texts, EpisodeRules::default());
        let text = |pos: u64| messages.iter().find(|m| m.pos == pos).map(|m| m.text.clone());
        let found = tasks(&cards, &eps, &[], &text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stage, "in_progress");
        assert!(found[0].checkpoint.contains("write_file on a.txt") && found[0].checkpoint.contains("b.txt"), "{}", found[0].checkpoint);
        assert!(found[0].checkpoint.contains("Teen files banao"));
        let updates = vec![
            TaskUpdate { goal: "Create a, b, c".into(), stage: "in_progress".into(), checkpoint: "a done".into(), pos: 1 },
            TaskUpdate { goal: "create a, b, c".into(), stage: "in_progress".into(), checkpoint: "a and b done, c next".into(), pos: 3 },
        ];
        let found = tasks(&cards, &eps, &updates, &text);
        let current = active(&found).unwrap();
        assert_eq!((current.source.as_str(), current.checkpoint.as_str()), ("model", "a and b done, c next"));
    }

    #[test]
    fn test_a_task_the_model_closed_is_not_shown_again() {
        let messages = vec![
            msg(0, "user", "a.txt aur b.txt banao", &[], &[]),
            msg(1, "assistant", "dono bana di", &["write_file"], &["a.txt", "b.txt"]),
        ];
        let cards = build_cards(&messages);
        let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
        let eps = episodes(&cards, &texts, EpisodeRules::default());
        let updates = vec![TaskUpdate { goal: "two files".into(), stage: "done".into(), checkpoint: "both written".into(), pos: 1 }];
        let found = tasks(&cards, &eps, &updates, &|_| None);
        assert!(active(&found).is_none());
        assert!(active(&tasks(&cards, &eps, &[], &|_| None)).is_some());
    }

    #[test]
    fn test_no_task_without_changes() {
        let messages = vec![msg(0, "user", "notes.txt padho", &[], &[]), msg(1, "assistant", "padh liya", &["read_text_file"], &["notes.txt"])];
        let cards = build_cards(&messages);
        let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
        let eps = episodes(&cards, &texts, EpisodeRules::default());
        assert!(tasks(&cards, &eps, &[], &|_| None).is_empty());
    }
}
