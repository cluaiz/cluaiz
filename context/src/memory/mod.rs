//! The Chat Map v1: cards for every message, a BM25 index over them, and recall of earlier
//! messages that are no longer in the prompt. Pure: the API reads the chat's event log and
//! passes the messages in.

pub mod card;
pub mod episode;
pub mod index;
pub mod recall;

pub use card::{build_cards, Card, Entities, MessageIn};
pub use episode::{episodes, Episode, EpisodeRules};
pub use index::CardIndex;
pub use recall::{recall, time_window, Hit, Window};
