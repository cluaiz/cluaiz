//! Projects actual agent events into display entries. This is not a second agent controller.

use super::display::Event;
use cluaiz_terminal::transcript::Role;
use cluaiz_terminal::{App, Approval, ApprovalResponse};
use std::sync::mpsc;

pub struct State {
    pub app: App,
    pub busy: bool,
    pub finished: bool,
    pub approval: Option<mpsc::SyncSender<ApprovalResponse>>,
    sequence: u64,
    answer_id: String,
    reasoning_id: String,
    tool_id: String,
    answer: String,
    reasoning: String,
    tool: String,
    tool_name: String,
    progress_id: String,
    tool_preparing: bool,
    thinking: bool,
    /// The context after the last answer (tokens, size): the live footer adds the tokens being generated
    context: Option<(usize, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_terminal::{Limits, Metadata};

    fn state() -> State {
        State::new(App::new(Metadata::default(), Limits::default()))
    }
    fn text(state: &mut State) -> String {
        state
            .app
            .transcript
            .visible(120, 60)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn shell_output_is_shown_whole_and_folds() {
        let mut state = state();
        let output: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        state.apply(Event::Shell { command: "ls".into(), ok: true, output });
        let view = text(&mut state);
        assert!(view.contains("! ls · 30 lines"), "{view}");
        assert!(view.contains("line 1") && view.contains("line 30"), "{view}");
        state.app.transcript.toggle_thinking();
        assert!(!text(&mut state).contains("line 30"));
    }

    #[test]
    fn the_footer_shows_the_context_and_speed_live() {
        let mut state = state();
        state.apply(Event::Context { used: 1000, size: 8000, speed: Some("20.0 tok/s · 3.0s".into()) });
        assert_eq!(state.app.context_usage.as_deref(), Some("Context 12% · 1000 / 8000 · 20.0 tok/s · 3.0s"));
        state.apply(Event::GenerationProgress { tokens: 50, seconds: 2, millis: 2000 });
        assert_eq!(state.app.context_usage.as_deref(), Some("Context 13% · 1050 / 8000 · 25.0 tok/s · 2s"));
    }

    #[test]
    fn test_progress_follows_thinking_and_tool_in_conversation_order() {
        let mut state = state();
        state.apply(Event::User("question".into()));
        state.apply(Event::StartAnswer);
        state.apply(Event::Reasoning("reasoning body".into()));
        state.apply(Event::GenerationProgress {
            tokens: 12,
            seconds: 2,
            millis: 2000,
        });
        let rows = state.app.transcript.visible_animated(
            100,
            60,
            cluaiz_terminal::Theme::Auto.palette(),
            Some("⠋"),
        );
        let view = rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(view.contains("⠋ ▾ Thinking · 12 tokens · 2s"), "{view}");
        assert!(view.contains("reasoning body"));
        assert!(!view.contains("Preparing response"));
        state.apply(Event::PreparingTool(
            "Preparing search · 18 bytes · 3s".into(),
        ));
        assert!(!text(&mut state).contains("reasoning body"));
        state.apply(Event::ToolStart("search".into(), "needle".into()));
        assert!(text(&mut state).contains("needle"));
        state.apply(Event::ToolEnd {
            ok: true,
            summary: "1 match".into(),
            content: "retained result".into(),
        });
        state.apply(Event::StartAnswer);
        state.apply(Event::Content("actual answer".into()));
        state.apply(Event::Ready("ready".into()));
        let view = text(&mut state);
        assert_eq!(view.matches("search · Result: 1 match").count(), 1);
        assert!(view.find("question").unwrap() < view.find("Thinking").unwrap());
        assert!(view.find("Thinking").unwrap() < view.find("search · Result").unwrap());
        assert!(view.find("search · Result").unwrap() < view.find("actual answer").unwrap());
        assert!(!view.contains("Preparing search"));
        assert!(!view.contains("Writing response"));
        assert!(!view.contains("retained result"));
        state.app.transcript.toggle_thinking();
        assert!(text(&mut state).contains("retained result"));
    }

    #[test]
    fn test_all_reasoning_chunks_close_on_answer_ready_or_error_and_can_reopen() {
        for ending in [
            Event::Content("answer".into()),
            Event::Ready("ready".into()),
            Event::Error("stopped".into()),
        ] {
            let mut state = state();
            state.apply(Event::StartAnswer);
            state.apply(Event::Reasoning(format!(
                "{}reasoning-tail-marker",
                "part ".repeat(7000)
            )));
            assert!(text(&mut state).contains("reasoning-tail-marker"));
            state.apply(ending);
            assert!(!text(&mut state).contains("reasoning-tail-marker"));
            state.app.transcript.toggle_thinking();
            assert!(text(&mut state).contains("reasoning-tail-marker"));
        }
    }

    #[test]
    fn test_deltas_update_one_answer_and_failed_tool_has_no_success_label() {
        let mut state = state();
        state.apply(Event::StartAnswer);
        state.apply(Event::Content("hello ".into()));
        state.apply(Event::Content("नमस्ते".into()));
        state.apply(Event::ToolStart("run_command".into(), "exit 7".into()));
        state.apply(Event::ToolEnd {
            ok: false,
            summary: "Exit code 7".into(),
            content: String::new(),
        });
        let view = text(&mut state);
        assert_eq!(view.matches("hello नमस्ते").count(), 1);
        assert!(view.contains("Error: Exit code 7"));
        assert!(!view.contains("Result:"));
        state.apply(Event::Ready("Ask before changes".into()));
        assert!(!state.busy);
        assert_eq!(state.app.metadata.status, "Ask before changes");
    }

    #[test]
    fn test_closing_pending_approval_sends_denial() {
        let mut state = state();
        let (reply, receiver) = mpsc::sync_channel(1);
        state.apply(Event::Approval {
            tool: "write_file".into(),
            reason: "file".into(),
            reply,
        });
        state.deny_pending();
        assert_eq!(receiver.recv().unwrap(), ApprovalResponse::Deny);
    }

    #[test]
    fn test_tool_only_turn_does_not_insert_an_empty_assistant_entry() {
        let mut state = state();
        state.apply(Event::StartAnswer);
        state.apply(Event::Content("\n\n".into()));
        state.apply(Event::ToolStart("read_file".into(), "README.md".into()));
        let view = text(&mut state);
        assert!(view.contains("read_file"));
        assert!(!view.contains("Cluaiz"), "{view}");
    }

    #[test]
    fn test_actual_event_roles_and_collapsed_reasoning_keep_answer_separate() {
        let mut state = state();
        state.apply(Event::User("question".into()));
        state.apply(Event::StartAnswer);
        state.apply(Event::Reasoning("reasoning detail".into()));
        state.apply(Event::Content("final answer".into()));
        state.apply(Event::Error("operation failed".into()));
        let view = text(&mut state);
        assert!(view.contains("> You"));
        assert!(view.contains("final answer"));
        assert!(!view.contains("Cluaiz"));
        assert!(view.contains("! Error"));
        assert!(!view.contains("reasoning detail"));
        state.app.transcript.toggle_thinking();
        assert!(text(&mut state).contains("reasoning detail"));
    }
}

impl State {
    pub fn new(app: App) -> Self {
        Self {
            app,
            busy: true,
            finished: false,
            approval: None,
            sequence: 0,
            answer_id: String::new(),
            reasoning_id: String::new(),
            tool_id: String::new(),
            answer: String::new(),
            reasoning: String::new(),
            tool: String::new(),
            tool_name: String::new(),
            progress_id: String::new(),
            tool_preparing: false,
            thinking: false,
            context: None,
        }
    }

    fn id(&mut self, prefix: &str) -> String {
        self.sequence += 1;
        format!("{prefix}-{}", self.sequence)
    }

    fn active(&mut self, id: &str) {
        self.app.transcript.set_active(Some(&format!("{id}-0")));
    }

    fn stop_progress(&mut self) {
        self.app
            .transcript
            .remove(&format!("{}-0", self.progress_id));
        self.progress_id.clear();
        self.app.transcript.set_active(None);
    }

    fn progress(&mut self, label: &str) {
        if self.progress_id.is_empty() {
            self.progress_id = self.id("progress");
        }
        Self::entry(&mut self.app, &self.progress_id, Role::Notice, label, "");
        self.active(&self.progress_id.clone());
    }

    fn end_thinking(&mut self) {
        if self.thinking {
            Self::entry(
                &mut self.app,
                &self.reasoning_id,
                Role::Thinking,
                "Thinking",
                &self.reasoning,
            );
            self.thinking = false;
        }
    }

    /// Splits large source text at UTF-8 boundaries so it is not silently clipped by entry limits.
    fn entry(app: &mut App, id: &str, role: Role, label: &str, body: &str) {
        const DISPLAY_CHUNK: usize = 32 * 1024;
        let mut rest = body;
        let mut part = 0;
        loop {
            let mut end = rest.len().min(DISPLAY_CHUNK);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            let key = format!("{id}-{part}");
            if let Err(error) = app.transcript.upsert_role(&key, role, label, &rest[..end]) {
                app.notice = Some(error.into());
                break;
            }
            rest = &rest[end..];
            if rest.is_empty() {
                break;
            }
            part += 1;
        }
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::StartAnswer => {
                self.end_thinking();
                self.stop_progress();
                self.tool_preparing = false;
                self.answer_id = self.id("answer");
                self.reasoning_id = self.id("reasoning");
                self.answer.clear();
                self.reasoning.clear();
                self.progress("Preparing response");
            }
            Event::Content(text) => {
                if text.trim().is_empty() && self.answer.is_empty() {
                    return;
                }
                self.end_thinking();
                self.stop_progress();
                self.answer.push_str(&text);
                if !self.answer.trim().is_empty() {
                    Self::entry(
                        &mut self.app,
                        &self.answer_id,
                        Role::Assistant,
                        "Cluaiz",
                        &self.answer,
                    );
                    self.progress("Writing response");
                }
            }
            Event::Reasoning(text) => {
                self.stop_progress();
                self.thinking = true;
                self.reasoning.push_str(&text);
                Self::entry(
                    &mut self.app,
                    &self.reasoning_id,
                    Role::ThinkingRunning,
                    "Thinking",
                    &self.reasoning,
                );
                self.active(&self.reasoning_id.clone());
            }
            Event::Phase(text) => {
                self.progress(&text);
            }
            Event::GenerationProgress { tokens, seconds, millis } => {
                if let Some((used, size)) = self.context {
                    self.app.context_usage = Some(context_line(used + tokens, size, live_speed(tokens, millis), seconds));
                }
                if !self.tool_preparing {
                    if !self.reasoning.is_empty() && self.answer.trim().is_empty() {
                        Self::entry(
                            &mut self.app,
                            &self.reasoning_id,
                            Role::ThinkingRunning,
                            &format!("Thinking · {tokens} tokens · {seconds}s"),
                            &self.reasoning,
                        );
                        self.active(&self.reasoning_id.clone());
                    } else {
                        self.progress(&format!(
                            "{} · {tokens} tokens · {seconds}s",
                            if self.answer.is_empty() {
                                "Generating"
                            } else {
                                "Writing"
                            }
                        ));
                    }
                }
            }
            Event::PreparingTool(label) => {
                self.end_thinking();
                self.stop_progress();
                if !self.tool_preparing {
                    self.tool_id = self.id("tool");
                }
                self.tool_preparing = true;
                self.tool_name = label;
                Self::entry(
                    &mut self.app,
                    &self.tool_id,
                    Role::ToolRunning,
                    &self.tool_name,
                    "",
                );
                self.active(&self.tool_id.clone());
            }
            Event::Shell { command, ok, output } => {
                // All of the output, open; its header (or Ctrl+O) folds it
                let id = self.id("shell");
                let lines = output.lines().count();
                let label = format!("! {command} · {}{lines} lines", if ok { "" } else { "failed · " });
                Self::entry(&mut self.app, &id, Role::Tool, &label, output.trim_end());
                self.app.transcript.expand_prefix(&format!("{id}-"));
            }
            Event::Message(label, body) => {
                let id = self.id("message");
                Self::entry(&mut self.app, &id, Role::Notice, &label, &body);
            }
            Event::User(body) => {
                self.stop_progress();
                let id = self.id("user");
                Self::entry(&mut self.app, &id, Role::User, "You", &body);
            }
            Event::Error(body) => {
                self.end_thinking();
                self.stop_progress();
                if self.tool_preparing {
                    Self::entry(
                        &mut self.app,
                        &self.tool_id,
                        Role::Notice,
                        "Tool call not executed",
                        &body,
                    );
                    self.tool_preparing = false;
                }
                let id = self.id("error");
                Self::entry(&mut self.app, &id, Role::Error, "Error", &body);
            }
            Event::ToolStart(name, target) => {
                self.end_thinking();
                self.stop_progress();
                if !self.tool_preparing {
                    self.tool_id = self.id("tool");
                }
                self.tool_preparing = false;
                self.tool_name = name;
                self.tool = target;
                Self::entry(
                    &mut self.app,
                    &self.tool_id,
                    Role::ToolRunning,
                    &self.tool_name,
                    &format!("{}\nPreparing", self.tool),
                );
                self.active(&self.tool_id.clone());
            }
            Event::Diff(diff) => {
                self.tool.push_str(&format!("\nProposed change:\n{diff}"));
                Self::entry(
                    &mut self.app,
                    &self.tool_id,
                    Role::ToolRunning,
                    &self.tool_name,
                    &self.tool,
                );
            }
            Event::ToolEnd {
                ok,
                summary,
                content,
            } => {
                self.app.transcript.set_active(None);
                self.tool.push_str(&format!(
                    "\n{}: {summary}",
                    if ok { "Result" } else { "Error" }
                ));
                if !content.is_empty() {
                    self.tool.push_str(&format!("\n{content}"));
                }
                Self::entry(
                    &mut self.app,
                    &self.tool_id,
                    Role::Tool,
                    &format!(
                        "{} · {}: {summary}",
                        self.tool_name,
                        if ok { "Result" } else { "Error" }
                    ),
                    &self.tool,
                );
            }
            Event::Approval {
                tool,
                reason,
                reply,
            } => {
                let request = Approval {
                    title: tool,
                    detail: reason,
                };
                if self.app.request_approval(request).is_ok() {
                    self.approval = Some(reply);
                } else {
                    let _ = reply.send(ApprovalResponse::Deny);
                }
            }
            Event::Themes => self.app.open_themes(),
            Event::Permission(strict) => self.app.strict = strict,
            Event::Ready(status) => {
                self.end_thinking();
                self.stop_progress();
                if self.tool_preparing {
                    Self::entry(
                        &mut self.app,
                        &self.tool_id,
                        Role::Notice,
                        "Tool preparation stopped",
                        "",
                    );
                    self.tool_preparing = false;
                }
                self.busy = false;
                self.app.activity = None;
                self.app.metadata.status = status;
            }
            Event::Usage(usage) => {
                self.context = None;
                self.app.context_usage = Some(usage)
            }
            Event::Context { used, size, speed } => {
                self.context = Some((used, size));
                let percent = used * 100 / size.max(1);
                self.app.context_usage = Some(match speed {
                    Some(speed) => format!("Context {percent}% · {used} / {size} · {speed}"),
                    None => format!("Context {percent}% · {used} / {size}"),
                });
            }
            Event::Finished => {
                self.end_thinking();
                self.finished = true;
            }
        }
    }

    pub fn deny_pending(&mut self) {
        if let Some(reply) = self.approval.take() {
            let _ = reply.send(ApprovalResponse::Deny);
        }
    }
}

/// The footer while an answer runs: the context grows by the tokens generated so far
fn context_line(used: usize, size: usize, speed: Option<f64>, seconds: u64) -> String {
    let percent = used * 100 / size.max(1);
    match speed {
        Some(speed) => format!("Context {percent}% · {used} / {size} · {speed:.1} tok/s · {seconds}s"),
        None => format!("Context {percent}% · {used} / {size} · {seconds}s"),
    }
}

/// Tokens per second so far; none in the first half second, when it means nothing yet
fn live_speed(tokens: usize, millis: u64) -> Option<f64> {
    (millis >= 500 && tokens > 0).then(|| tokens as f64 * 1000.0 / millis as f64)
}
