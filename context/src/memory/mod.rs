//! The Chat Map v1: cards for every message, a BM25 index over them, and recall of earlier
//! messages that are no longer in the prompt. Pure: the API reads the chat's event log and
//! passes the messages in.

pub mod card;
pub mod episode;
pub mod graph;
pub mod index;
pub mod recall;
pub mod task;
pub mod time;

pub use card::{build_cards, Card, Enrichment, Entities, MessageIn};
pub use episode::{episodes, Episode, EpisodeRules, EpisodeSummary};
pub use graph::{build_graph, expand, Edge, Graph, Node};
pub use index::CardIndex;
pub use recall::{asks_about_tool_run, recall, Hit};
pub use time::{time_window, time_window_in, Window};
pub use task::{active as active_task, tasks, Task, TaskUpdate};
