//! Presentation-only terminal primitives. Callers own execution and persistence.

#![forbid(unsafe_code)]

pub mod app;
pub mod input;
pub mod keymap;
pub mod model;
pub mod render;
pub mod terminal;
pub mod theme;
pub mod transcript;

mod sanitize;

pub use app::{Action, App};
pub use model::{Limits, Metadata};
pub use render::render;
pub use terminal::Screen;
pub use theme::Theme;
