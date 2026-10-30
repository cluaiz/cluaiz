//! Terminal output of a chat: the streamed answer on stdout, everything else (plan, speed) on stderr,
//! so `cluaiz run model "prompt" > answer.txt` writes only the answer.

use std::io::{IsTerminal, Write};

use colored::Colorize;

use super::markdown::MarkdownStream;
use super::{Session, TurnReport};

#[derive(PartialEq)]
enum Phase {
    Start,
    Thinking,
    Answer,
}

/// Prints one streamed answer. Thinking is shown dimmed between "Thinking..." and "...done thinking."
/// (like ollama), and only when stdout is a terminal: piped output is the answer alone. On a terminal
/// the answer's Markdown is drawn formatted.
pub struct StreamPrinter {
    show_thinking: bool,
    markdown: Option<MarkdownStream>,
    phase: Phase,
    ends_with_newline: bool,
    wrote: bool,
}

impl StreamPrinter {
    pub fn new(show_thinking: bool) -> Self {
        let terminal = std::io::stdout().is_terminal();
        Self {
            show_thinking: show_thinking && terminal,
            markdown: terminal.then(MarkdownStream::new),
            phase: Phase::Start,
            ends_with_newline: true,
            wrote: false,
        }
    }

    pub fn reasoning(&mut self, text: &str) {
        if !self.show_thinking || text.is_empty() {
            return;
        }
        if self.phase == Phase::Start {
            self.write(&format!("{}\n", "Thinking...".dimmed()));
            self.phase = Phase::Thinking;
            self.write(&text.trim_start().dimmed().to_string());
        } else {
            self.write(&text.dimmed().to_string());
        }
        self.ends_with_newline = text.ends_with('\n');
    }

    pub fn content(&mut self, text: &str) {
        // Blank lines between the thinking and the answer are not printed
        let text = if self.phase == Phase::Answer { text } else { text.trim_start() };
        if text.is_empty() {
            return;
        }
        if self.phase == Phase::Thinking {
            self.close_thinking();
        }
        self.phase = Phase::Answer;
        match self.markdown.as_mut() {
            Some(md) => {
                md.push(text);
                self.wrote = true;
            }
            None => self.write(text),
        }
        self.ends_with_newline = text.ends_with('\n');
    }

    /// Ends the answer on a fresh line.
    pub fn finish(&mut self) {
        if self.phase == Phase::Thinking {
            self.close_thinking();
        }
        if let Some(md) = self.markdown.as_mut() {
            md.finish();
            self.ends_with_newline = true;
        }
        if self.wrote && !self.ends_with_newline {
            self.write("\n");
        }
        self.phase = Phase::Start;
        self.ends_with_newline = true;
        self.wrote = false;
    }

    fn close_thinking(&mut self) {
        let lead = if self.ends_with_newline { "" } else { "\n" };
        self.write(&format!("{}{}\n\n", lead, "...done thinking.".dimmed()));
        self.ends_with_newline = true;
    }

    fn write(&mut self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(text.as_bytes());
        let _ = out.flush();
        self.wrote = true;
    }
}

/// Plan the model was loaded with (stderr). `-v` adds the full planner banner.
pub async fn plan(session: &Session) {
    let loaded = if session.load_ms > 0 { format!(" in {:.2} s", session.load_ms as f64 / 1000.0) } else { String::new() };
    eprintln!("{} {}{}", "Loaded".green().bold(), session.model_id.bold(), loaded);
    for line in session.plan.summary().lines() {
        eprintln!("  {}", line.dimmed());
    }
    if session.options.verbose {
        if let Some(banner) = session.host.plan_report.lock().await.as_ref() {
            eprintln!("{}", banner);
        }
    }
}

/// llama.cpp's timings of the last answer and what the thinking budget did (stderr).
pub fn stats(report: &TurnReport) {
    if let Some(t) = &report.timings {
        eprintln!(
            "{}",
            format!(
                "prompt eval  {:>6} tokens  {:>8.2} s  {:>8.2} tokens/s\neval         {:>6} tokens  {:>8.2} s  {:>8.2} tokens/s",
                t.prompt_n, t.prompt_ms / 1000.0, t.prompt_per_second,
                t.predicted_n, t.predicted_ms / 1000.0, t.predicted_per_second,
            )
            .dimmed()
        );
    }
    if let Some(th) = &report.thinking {
        let effort = serde_json::to_value(th.effort).map(|v| crate::cli::keys::display(&v)).unwrap_or_default();
        let capped = if th.capped { ", lowered to fit the context" } else { "" };
        eprintln!(
            "{}",
            format!(
                "thinking     effort {}, budget {} tokens{}, used {}, ended by {}",
                effort, th.budget_tokens, capped, th.used_tokens, th.ended_by
            )
            .dimmed()
        );
    }
    if report.cancelled {
        eprintln!("{}", "stopped by Ctrl+C".dimmed());
    }
}
