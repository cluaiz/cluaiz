//! Loading indicator for waits with no progress of their own (network lookups, model load, benchmark
//! runs). Drawn on stderr and only on a terminal, so piped output never gets it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

/// Spins until dropped (or `stop`), then clears its line.
pub struct Spinner(ProgressBar);

pub const TICK_INTERVAL: Duration = Duration::from_millis(100);

/// Log lines on stderr (`-v`) are the progress there: a spinner on the same stream would share
/// their line, so it is not drawn
static LOGS_ON_STDERR: AtomicBool = AtomicBool::new(false);

pub fn logs_on_stderr(on: bool) {
    LOGS_ON_STDERR.store(on, Ordering::Relaxed);
}

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
        let target = if LOGS_ON_STDERR.load(Ordering::Relaxed) { ProgressDrawTarget::hidden() } else { ProgressDrawTarget::stderr() };
        let pb = ProgressBar::with_draw_target(None, target);
        pb.set_style(style().clone());
        pb.set_message(message.into());
        pb.enable_steady_tick(TICK_INTERVAL);
        Self(pb)
    }
}

impl Spinner {
    pub fn set_message(&self, message: impl Into<String>) {
        self.0.set_message(message.into());
    }

    /// Shows the live steps of chat-model loads in this process after `label` (`Loading model:
    /// reading weights 37%`) until the returned follower is dropped
    pub fn follow_loads(&self, label: &str) -> LoadFollower {
        use tokio::sync::broadcast::error::TryRecvError;
        let bar = self.0.clone();
        let label = label.to_string();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let stopped = std::sync::Arc::clone(&stop);
        let mut notices = cluaiz_runtime::load_events::subscribe();
        // The load runs synchronously on a runtime thread, so the steps are drawn from a thread of
        // their own
        let thread = std::thread::spawn(move || loop {
            match notices.try_recv() {
                Ok(event) => bar.set_message(format!("{label}: {}", event.describe())),
                Err(TryRecvError::Lagged(_)) => {}
                Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Empty) if stopped.load(Ordering::Relaxed) => break,
                Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(50)),
            }
        });
        LoadFollower { stop, thread: Some(thread) }
    }
}

/// Stops drawing load steps when dropped
pub struct LoadFollower {
    stop: std::sync::Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for LoadFollower {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}
