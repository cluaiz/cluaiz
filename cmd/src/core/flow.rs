use crate::core::state::AppState;
use crate::theme::Theme;
use ratatui::{layout::Rect, Frame};

/// 🛰️ cluaiz FLOW ENGINE (Legacy/Passthrough)
/// In the Streaming Era, FlowEngine only manages terminal restoration.
pub struct FlowEngine { }

impl FlowEngine {
    pub fn new() -> color_eyre::Result<Self> {
        // Just return a dummy instance; we are now in native console mode.
        Ok(Self { })
    }

    pub fn restore() -> color_eyre::Result<()> {
        let _ = crossterm::terminal::disable_raw_mode();
        Ok(())
    }

    pub fn draw(
        &mut self,
        _state: &mut AppState,
        _theme: &Theme,
        _render_fn: impl FnOnce(&mut AppState, &Theme, Rect, &mut Frame),
    ) {
        // No-op: We print directly to the terminal now.
    }

    pub fn clear(&mut self) {
        let _ = print!("\x1B[2J\x1B[1;1H"); // ANSI clear screen
    }
}
