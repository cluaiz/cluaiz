//! Plain and screen adapters for the existing agent. No model/tool execution lives here.

use crate::chat::{
    output::Output,
    print::{CallFilter, StreamPrinter},
};
use cluaiz_terminal::ApprovalResponse;
use colored::Colorize;
use std::{
    io::{BufRead, IsTerminal, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Instant,
};

const CHUNK_BYTES: usize = 4096;

pub enum Event {
    StartAnswer,
    Content(String),
    Reasoning(String),
    Phase(String),
    Message(String, String),
    ToolStart(String, String),
    Diff(String),
    ToolEnd {
        ok: bool,
        summary: String,
        content: String,
    },
    Approval {
        tool: String,
        reason: String,
        reply: mpsc::SyncSender<ApprovalResponse>,
    },
    Themes,
    Ready(String),
    Finished,
}

#[derive(Clone, Default)]
pub struct Display {
    sender: Option<mpsc::SyncSender<Event>>,
    closing: Arc<AtomicBool>,
    interrupted: Arc<AtomicBool>,
}

impl Display {
    pub fn screen(sender: mpsc::SyncSender<Event>, closing: Arc<AtomicBool>) -> Self {
        Self {
            sender: Some(sender),
            closing,
            interrupted: Arc::default(),
        }
    }

    pub fn closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }
    pub fn screen_enabled(&self) -> bool {
        self.sender.is_some()
    }

    pub fn interrupt(&self) {
        self.interrupted.store(true, Ordering::SeqCst);
        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
    }

    pub fn interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    pub fn begin_turn(&self) {
        self.interrupted.store(false, Ordering::SeqCst);
    }

    pub fn emit(&self, event: Event) {
        if let Some(sender) = &self.sender {
            if sender.send(event).is_err() {
                self.closing.store(true, Ordering::SeqCst);
                cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
            }
            return;
        }
        match event {
            Event::Message(label, body) => eprintln!("{label}: {body}"),
            Event::ToolStart(name, target) => {
                eprintln!("{} {} {}", "●".cyan(), name.bold(), target)
            }
            Event::Diff(text) => eprintln!("{text}"),
            Event::Themes => {
                eprintln!("Use an interactive terminal with --ui tui to select a theme.")
            }
            Event::ToolEnd { ok, summary, .. } => eprintln!(
                "  {} {}",
                "⎿".dimmed(),
                if ok { summary.dimmed() } else { summary.red() }
            ),
            _ => {}
        }
    }

    pub fn output(&self, thinking: bool) -> Box<dyn Output> {
        if self.sender.is_some() {
            self.emit(Event::StartAnswer);
            Box::new(ScreenOutput {
                display: self.clone(),
                filter: CallFilter::default(),
                text: String::new(),
                reasoning: String::new(),
                show_thinking: thinking,
                started: Instant::now(),
            })
        } else {
            Box::new(StreamPrinter::new(thinking).hiding_tool_calls())
        }
    }

    pub fn approve(&self, tool: &str, reason: &str) -> ApprovalResponse {
        if self.closing() {
            return ApprovalResponse::Deny;
        }
        if self.sender.is_some() {
            let (reply, receiver) = mpsc::sync_channel(1);
            self.emit(Event::Approval {
                tool: tool.into(),
                reason: reason.into(),
                reply,
            });
            loop {
                if self.closing() {
                    return ApprovalResponse::Deny;
                }
                match receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(answer) => return answer,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => return ApprovalResponse::Deny,
                }
            }
        }
        if !std::io::stdin().is_terminal() {
            return ApprovalResponse::Deny;
        }
        eprint!("  Allow {tool} {reason}? [y]es / [n]o / [a]lways this session, or instructions: ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).is_err() {
            return ApprovalResponse::Deny;
        }
        ApprovalResponse::parse(&line)
    }
}

struct ScreenOutput {
    display: Display,
    filter: CallFilter,
    text: String,
    reasoning: String,
    show_thinking: bool,
    started: Instant,
}

impl ScreenOutput {
    fn flush(&mut self) {
        if !self.text.is_empty() {
            self.display
                .emit(Event::Content(std::mem::take(&mut self.text)));
        }
        if !self.reasoning.is_empty() {
            self.display
                .emit(Event::Reasoning(std::mem::take(&mut self.reasoning)));
        }
    }
}

impl Output for ScreenOutput {
    fn content(&mut self, text: &str) {
        self.text.push_str(&self.filter.pass(text));
        if self.text.len() >= CHUNK_BYTES {
            self.flush();
        }
    }
    fn reasoning(&mut self, text: &str) {
        if self.show_thinking {
            self.reasoning.push_str(text);
        }
        if self.reasoning.len() >= CHUNK_BYTES {
            self.flush();
        }
    }
    fn tick(&mut self) {
        self.flush();
        if self.display.closing() || self.display.interrupted() {
            cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
        }
        let phase = match cluaiz_core::live::call() {
            Some((name, args)) => format!(
                "Preparing {name} · {} bytes · {}s",
                args.len(),
                self.started.elapsed().as_secs()
            ),
            None => format!(
                "Generating · {} tokens · {}s",
                cluaiz_core::live::tokens(),
                self.started.elapsed().as_secs()
            ),
        };
        self.display.emit(Event::Phase(phase));
    }
    fn finish(&mut self) {
        self.text.push_str(&self.filter.finish());
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streamed_tool_markup_is_hidden_by_the_existing_filter() {
        let (sender, receiver) = mpsc::sync_channel(16);
        let display = Display::screen(sender, Arc::default());
        let mut output = display.output(false);
        for chunk in [
            "Reading. <tool_",
            "call>{\"name\":\"read_file\",\"arguments\":{\"path\":\"a\"}}",
            "</tool_call> Done.",
        ] {
            output.content(chunk);
        }
        output.reasoning("hidden thinking");
        output.finish();
        let visible = receiver
            .try_iter()
            .filter_map(|event| match event {
                Event::Content(text) => Some(text),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(visible, "Reading.  Done.");
        assert!(!visible.contains("tool_call"));
        assert!(!visible.contains("hidden thinking"));
    }

    #[test]
    fn test_disconnected_screen_never_implicitly_approves() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let display = Display::screen(sender, Arc::default());
        assert_eq!(
            display.approve("write_file", "file"),
            ApprovalResponse::Deny
        );
        assert!(display.closing());
        cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, Ordering::SeqCst);
    }
}
