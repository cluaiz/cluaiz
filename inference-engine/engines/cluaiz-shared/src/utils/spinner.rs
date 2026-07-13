use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

pub struct cluaizSpinner {
    pb: Option<ProgressBar>,
}

impl cluaizSpinner {
    pub fn new() -> Self {
        Self { pb: None }
    }

    /// Starts the global spinner with a specific message.
    /// In debug mode, this doesn't run so it doesn't conflict with verbose logs.
    pub fn start(&mut self, message: &str) {
        let pb = ProgressBar::with_draw_target(None, indicatif::ProgressDrawTarget::stdout());
        pb.enable_steady_tick(Duration::from_millis(120));
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message(message.to_string());
        self.pb = Some(pb);
    }

    /// Stops the global spinner and optionally prints a final success message.
    pub fn stop(&mut self, final_message: Option<&str>) {
        if let Some(pb) = self.pb.take() {
            if let Some(msg) = final_message {
                pb.finish_with_message(msg.to_string());
            } else {
                pb.finish_and_clear();
            }
        }
    }
}
