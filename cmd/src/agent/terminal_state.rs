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
        assert!(view.contains("● Cluaiz"));
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
        }
    }

    fn id(&mut self, prefix: &str) -> String {
        self.sequence += 1;
        format!("{prefix}-{}", self.sequence)
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
                self.answer_id = self.id("answer");
                self.reasoning_id = self.id("reasoning");
                self.answer.clear();
                self.reasoning.clear();
                self.app.metadata.status = "Generating…  Esc: interrupt".into();
            }
            Event::Content(text) => {
                self.answer.push_str(&text);
                if !self.answer.trim().is_empty() {
                    Self::entry(
                        &mut self.app,
                        &self.answer_id,
                        Role::Assistant,
                        "Cluaiz",
                        &self.answer,
                    );
                }
            }
            Event::Reasoning(text) => {
                self.reasoning.push_str(&text);
                Self::entry(
                    &mut self.app,
                    &self.reasoning_id,
                    Role::Thinking,
                    "Thinking",
                    &self.reasoning,
                );
            }
            Event::Phase(text) => self.app.metadata.status = text,
            Event::Message(label, body) => {
                let id = self.id("message");
                Self::entry(&mut self.app, &id, Role::Notice, &label, &body);
            }
            Event::User(body) => {
                let id = self.id("user");
                Self::entry(&mut self.app, &id, Role::User, "You", &body);
            }
            Event::Error(body) => {
                let id = self.id("error");
                Self::entry(&mut self.app, &id, Role::Error, "Error", &body);
            }
            Event::ToolStart(name, target) => {
                self.tool_id = self.id("tool");
                self.tool_name = name;
                self.tool = target;
                Self::entry(
                    &mut self.app,
                    &self.tool_id,
                    Role::Tool,
                    &self.tool_name,
                    &format!("{}\nPreparing", self.tool),
                );
            }
            Event::Diff(diff) => {
                self.tool.push_str(&format!("\nProposed change:\n{diff}"));
                Self::entry(
                    &mut self.app,
                    &self.tool_id,
                    Role::Tool,
                    &self.tool_name,
                    &self.tool,
                );
            }
            Event::ToolEnd {
                ok,
                summary,
                content,
            } => {
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
                    if ok { Role::Tool } else { Role::Error },
                    &self.tool_name,
                    &self.tool,
                );
            }
            Event::Approval {
                tool,
                reason,
                reply,
            } => {
                self.app.metadata.status = "Waiting for your permission".into();
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
            Event::Ready(status) => {
                self.busy = false;
                self.app.metadata.status = status;
            }
            Event::Finished => {
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
