//! The ledger of a conversation sent without a session id (any OpenAI-style client), kept in
//! memory only.
//!
//! The Engine puts a block in front of the current user message (tool hints, notes, project
//! items). A chat with a session id gets that block back on its later turns from its ledger on
//! disk; a conversation without one would lose it, so its earlier turns would render differently
//! on every turn and the KV cache could not be reused (each turn processed from token 0). This keeps
//! the ledger of the one conversation the Engine is serving, in RAM, the way llama-server keeps its
//! slot: the next turn of the same conversation renders the earlier turns as the model saw them.
//! Nothing is written to disk; a request from another conversation replaces it, and a restart
//! forgets it. A conversation is known by its system messages and first user message.

use super::types::ExternalMessage;
use cluaiz_agents::ChatLedger;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

static LIVE: Mutex<Option<(u64, ChatLedger)>> = Mutex::new(None);

/// The conversation `messages` belong to: its system messages and its first user message
pub fn conversation(messages: &[ExternalMessage]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for message in messages {
        let system = message.role.eq_ignore_ascii_case("system");
        if system || message.role.eq_ignore_ascii_case("user") {
            message.role.to_ascii_lowercase().hash(&mut hasher);
            message.content.text_content().hash(&mut hasher);
        }
        if !system && message.role.eq_ignore_ascii_case("user") {
            break;
        }
    }
    hasher.finish()
}

/// The ledger of this conversation, when it is the one kept
pub fn get(conversation: u64) -> Option<ChatLedger> {
    let live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    live.as_ref().filter(|(id, _)| *id == conversation).map(|(_, ledger)| ledger.clone())
}

/// Keeps `ledger` as the ledger of `conversation`, replacing any other conversation's
pub fn put(conversation: u64, ledger: ChatLedger) {
    *LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((conversation, ledger));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::chat::types::MessageContent;

    fn message(role: &str, text: &str) -> ExternalMessage {
        ExternalMessage { role: role.into(), content: MessageContent::Text(text.into()), ..Default::default() }
    }

    #[test]
    fn a_later_turn_of_the_same_conversation_finds_its_ledger_and_another_conversation_does_not() {
        let first = vec![message("system", "be brief"), message("user", "the code is BLUE-42")];
        let later = vec![
            message("system", "be brief"),
            message("user", "the code is BLUE-42"),
            message("assistant", "OK"),
            message("user", "what is the code?"),
        ];
        let other = vec![message("system", "be brief"), message("user", "hello")];
        assert_eq!(conversation(&first), conversation(&later));
        assert_ne!(conversation(&first), conversation(&other));

        let mut ledger = ChatLedger::default();
        ledger.set_turn(cluaiz_agents::TurnRecord { key: "k".into(), block: "<tools_hint>x</tools_hint>".into(), steps: vec![] });
        put(conversation(&first), ledger);
        assert!(get(conversation(&later)).is_some_and(|l| l.turns.len() == 1));
        assert!(get(conversation(&other)).is_none());
        // Another conversation replaces it
        put(conversation(&other), ChatLedger::default());
        assert!(get(conversation(&later)).is_none());
    }
}
