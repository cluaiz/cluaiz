//! Terminal output of a chat: the streamed answer on stdout, everything else (plan, speed) on stderr,
//! so `cluaiz run model "prompt" > answer.txt` writes only the answer.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

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
/// the answer's Markdown is drawn formatted. While nothing is printed (prompt reading, hidden thinking,
/// a tool call being written) a status line on stderr shows what the model is doing.
pub struct StreamPrinter {
    show_thinking: bool,
    markdown: Option<MarkdownStream>,
    /// Agent: `<tool_call>` blocks are not shown (each call gets its own row)
    calls: Option<CallFilter>,
    phase: Phase,
    ends_with_newline: bool,
    wrote: bool,
    /// Both stdout and stderr are a terminal: the status line can be drawn
    live: bool,
    status_shown: bool,
    started: Instant,
    last_output: Instant,
    ticks: usize,
}

impl StreamPrinter {
    pub fn new(show_thinking: bool) -> Self {
        let terminal = std::io::stdout().is_terminal();
        Self {
            show_thinking: show_thinking && terminal,
            markdown: terminal.then(MarkdownStream::new),
            calls: None,
            phase: Phase::Start,
            ends_with_newline: true,
            wrote: false,
            live: terminal && std::io::stderr().is_terminal(),
            status_shown: false,
            started: Instant::now(),
            last_output: Instant::now(),
            ticks: 0,
        }
    }

    /// Called a few times a second while generating: after a pause in the output, draws the status
    /// line (on a line of its own); the next output clears it.
    pub fn tick(&mut self) {
        self.ticks += 1;
        if !self.live || self.last_output.elapsed() < Duration::from_millis(700) {
            return;
        }
        if !self.status_shown {
            if let Some(md) = self.markdown.as_mut().filter(|_| self.phase == Phase::Answer) {
                md.break_line();
            } else if !self.ends_with_newline {
                self.write("\n");
            }
            self.ends_with_newline = true;
        }
        let spin = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][self.ticks % 10];
        let call = cluaiz_core::live::call()
            .or_else(|| self.calls.as_ref().and_then(CallFilter::partial))
            .filter(|_| self.calls.is_some());
        let what = match call {
            Some((name, arguments)) => format!("{} {}  {} chars", name.bold(), argument_preview(&arguments), arguments.len()),
            None => match cluaiz_core::live::tokens() {
                0 => "Reading the prompt".to_string(),
                n => format!("Writing  {n} tokens"),
            },
        };
        // One line only (a wrapped status could not be cleared): the plain text is cut to the width
        let cols = crossterm::terminal::size().map(|(c, _)| c as usize).unwrap_or(80);
        let what: String = what.chars().take(cols.saturating_sub(16)).collect();
        eprint!("\r\x1b[2K{} {}  {}", spin.cyan(), what, format!("{}s", self.started.elapsed().as_secs()).dimmed());
        let _ = std::io::stderr().flush();
        self.status_shown = true;
    }

    fn clear_status(&mut self) {
        self.last_output = Instant::now();
        if self.status_shown {
            eprint!("\r\x1b[2K");
            let _ = std::io::stderr().flush();
            self.status_shown = false;
        }
    }

    pub fn reasoning(&mut self, text: &str) {
        if !self.show_thinking || text.is_empty() {
            return;
        }
        self.clear_status();
        if self.phase == Phase::Start {
            self.write(&format!("{}\n", "Thinking...".dimmed()));
            self.phase = Phase::Thinking;
            self.write(&text.trim_start().dimmed().to_string());
        } else {
            self.write(&text.dimmed().to_string());
        }
        self.ends_with_newline = text.ends_with('\n');
    }

    /// Leaves `<tool_call>` blocks out of the shown answer.
    pub fn hiding_tool_calls(mut self) -> Self {
        self.calls = Some(CallFilter::default());
        self
    }

    pub fn content(&mut self, text: &str) {
        match self.calls.as_mut() {
            Some(calls) => {
                let shown = calls.pass(text);
                self.show(&shown);
            }
            None => self.show(text),
        }
    }

    fn show(&mut self, text: &str) {
        // Blank lines between the thinking and the answer are not printed
        let text = if self.phase == Phase::Answer { text } else { text.trim_start() };
        if text.is_empty() {
            return;
        }
        self.clear_status();
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
        self.clear_status();
        if let Some(rest) = self.calls.as_mut().map(CallFilter::finish) {
            self.show(&rest);
        }
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

/// Text of a stream with its `<tool_call>` blocks left out. A tail that may be the start of a block
/// is held back until the next text shows whether it is one.
#[derive(Default)]
pub(crate) struct CallFilter {
    held: String,
    inside: bool,
}

impl CallFilter {
    const OPEN: &'static str = cluaiz_core::tool_calls::OPEN;
    const CLOSE: &'static str = cluaiz_core::tool_calls::CLOSE;

    pub(crate) fn pass(&mut self, text: &str) -> String {
        self.held.push_str(text);
        let mut shown = String::new();
        loop {
            if self.inside {
                let Some(at) = self.held.find(Self::CLOSE) else { return shown };
                self.held.drain(..at + Self::CLOSE.len());
                self.inside = false;
            } else if let Some(at) = self.held.find(Self::OPEN) {
                shown.push_str(&self.held[..at]);
                self.held.drain(..at + Self::OPEN.len());
                self.inside = true;
            } else {
                let keep = (1..Self::OPEN.len()).rev().find(|&n| self.held.ends_with(&Self::OPEN[..n])).unwrap_or(0);
                let cut = self.held.len() - keep;
                shown.push_str(&self.held[..cut]);
                self.held.drain(..cut);
                return shown;
            }
        }
    }

    /// The call being written (inline form): its name and arguments' text so far
    pub(crate) fn partial(&self) -> Option<(String, String)> {
        if !self.inside {
            return None;
        }
        let name = string_value(&self.held, "name")?;
        let arguments = self.held.split_once("\"arguments\"").map(|(_, rest)| rest.to_string()).unwrap_or_default();
        Some((name, arguments))
    }

    /// What is still held back: shown when it was not a block after all
    pub(crate) fn finish(&mut self) -> String {
        let rest = std::mem::take(&mut self.held);
        std::mem::replace(&mut self.inside, false).then(String::new).unwrap_or(rest)
    }
}

/// The value of `"key": "..."` in JSON text that may still be incomplete
fn string_value(json: &str, key: &str) -> Option<String> {
    let after = json.split_once(&format!("\"{key}\""))?.1.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = after.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(if next == 'n' { ' ' } else { next });
                }
            }
            c => out.push(c),
        }
    }
    Some(out)
}

/// What a call works on, from its arguments so far: the path, command or pattern
fn argument_preview(arguments: &str) -> String {
    let value = ["path", "command", "pattern"].iter().find_map(|key| string_value(arguments, key)).unwrap_or_default();
    let mut short: String = value.chars().take(50).collect();
    if short.len() < value.len() {
        short.push('…');
    }
    short
}

/// Plan the model was loaded with (stderr). `-v` adds the full planner banner.
pub async fn plan(session: &Session) {
    let loaded = if session.load_ms > 0 { format!(" in {:.2} s", session.load_ms as f64 / 1000.0) } else { String::new() };
    match &session.remote {
        Some(remote) => eprintln!("{} {} {}{}", "Using".green().bold(), session.model_id.bold(), format!("in the running server at {}", remote.url).dimmed(), loaded),
        None => eprintln!("{} {}{}", "Loaded".green().bold(), session.model_id.bold(), loaded),
    }
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

#[cfg(test)]
mod tests {
    use super::CallFilter;

    #[test]
    fn tool_call_blocks_are_left_out() {
        let mut f = CallFilter::default();
        let mut shown = String::new();
        for piece in ["Reading it.\n<to", "ol_call>\n{\"name\": \"read_file\"}", "\n</tool_call>", " done <b"] {
            shown.push_str(&f.pass(piece));
        }
        shown.push_str(&f.finish());
        assert_eq!(shown, "Reading it.\n done <b");
    }
}
