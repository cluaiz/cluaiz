use cluaiz_terminal::{
    app::Suggestion,
    event::{Event, KeyCode, KeyEvent, KeyModifiers},
    render, Action, App, Approval, ApprovalResponse, Limits, Metadata,
};
use ratatui::{backend::TestBackend, Terminal};

fn app() -> App {
    App::new(Metadata::default(), Limits::default())
}
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|row| {
            (0..width)
                .map(|col| buffer[(col, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_approval_feedback_keeps_case_and_restores_busy_draft() {
    let mut app = app();
    app.set_draft("Next question नमस्ते").unwrap();
    app.request_approval(Approval {
        title: "edit_file".into(),
        detail: "src/main.rs".into(),
    })
    .unwrap();
    assert!(app
        .request_approval(Approval {
            title: "other".into(),
            detail: String::new()
        })
        .is_err());
    app.handle(Event::Paste("Keep My API Name".into()));
    assert_eq!(
        app.handle(key(KeyCode::Enter)),
        Action::Approval(ApprovalResponse::Feedback("Keep My API Name".into()))
    );
    assert_eq!(app.draft(), "Next question नमस्ते");
}

#[test]
fn test_long_approval_cannot_allow_hidden_operation() {
    let mut app = app();
    app.request_approval(Approval {
        title: "run_command".into(),
        detail: format!("{}\nEND-OF-COMMAND", "line\n".repeat(20)),
    })
    .unwrap();
    let first = draw(&mut app, 80, 24);
    assert!(first.contains("read more"));
    app.handle(Event::Paste("y".into()));
    assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
    assert!(app.approval_pending());
    for _ in 0..40 {
        app.handle(key(KeyCode::PageDown));
        draw(&mut app, 80, 24);
    }
    assert!(draw(&mut app, 80, 24).contains("END-OF-COMMAND"));
    app.handle(Event::Paste("y".into()));
    assert_eq!(
        app.handle(key(KeyCode::Enter)),
        Action::Approval(ApprovalResponse::Allow)
    );
}

#[test]
fn test_tiny_terminal_cannot_blindly_allow_and_escape_denies() {
    let mut app = app();
    app.request_approval(Approval {
        title: "write_file".into(),
        detail: "file".into(),
    })
    .unwrap();
    draw(&mut app, 10, 3);
    app.handle(Event::Paste("a".into()));
    assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
    assert_eq!(
        app.handle(key(KeyCode::Esc)),
        Action::Approval(ApprovalResponse::Deny)
    );
}

#[test]
fn test_file_completion_inserts_without_sending_and_preserves_middle_edit() {
    let mut app = app();
    app.set_draft("Read @fi").unwrap();
    app.set_suggestions(
        5,
        vec![Suggestion {
            value: "@file.rs".into(),
            description: "File reference".into(),
        }],
    );
    assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
    assert_eq!(app.draft(), "Read @file.rs");
    app.handle(key(KeyCode::Left));
    app.set_suggestions(
        5,
        vec![Suggestion {
            value: "@wrong".into(),
            description: String::new(),
        }],
    );
    assert_eq!(
        app.handle(key(KeyCode::Enter)),
        Action::Submit("Read @file.rs".into())
    );
}

#[test]
fn test_command_completion_dispatch_and_escape_focus() {
    let mut app = app();
    app.set_draft("/th").unwrap();
    let choices = vec![Suggestion {
        value: "/theme".into(),
        description: "Theme".into(),
    }];
    app.set_suggestions(0, choices.clone());
    assert_eq!(app.handle(key(KeyCode::Esc)), Action::None);
    assert_eq!(app.draft(), "/th");
    app.set_suggestions(0, choices);
    assert_eq!(
        app.handle(key(KeyCode::Enter)),
        Action::Submit("/theme".into())
    );
}

#[test]
fn test_oversized_completion_keeps_draft_without_dispatch() {
    let mut app = App::new(
        Metadata::default(),
        Limits {
            input_bytes: 5,
            ..Limits::default()
        },
    );
    app.set_draft("/th").unwrap();
    app.set_suggestions(
        0,
        vec![Suggestion {
            value: "/theme".into(),
            description: String::new(),
        }],
    );
    assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
    assert_eq!(app.draft(), "/th");
}
