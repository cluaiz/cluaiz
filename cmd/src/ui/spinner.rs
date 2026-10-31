//! Loading indicator for waits with no progress of their own (network lookups, model load, benchmark
//! runs). Drawn on stderr and only on a terminal, so piped output never gets it.

use std::sync::OnceLock;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

/// Spins until dropped (or `stop`), then clears its line.
pub struct Spinner(ProgressBar);

pub const TICK_INTERVAL: Duration = Duration::from_millis(100);

fn style() -> &'static ProgressStyle {
    static STYLE: OnceLock<ProgressStyle> = OnceLock::new();
    STYLE.get_or_init(|| {
        ProgressStyle::with_template("  {spinner:.cyan} {msg} {elapsed:.dim}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
    })
}

/// The screen renderer reuses the animation without starting a competing stderr writer.
pub fn frame(tick: u64) -> &'static str {
    style().get_tick_str(tick)
}

impl Spinner {
    pub fn start(message: impl Into<String>) -> Self {
        let pb = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
        pb.set_style(style().clone());
        pb.set_message(message.into());
        pb.enable_steady_tick(TICK_INTERVAL);
        Self(pb)
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}
