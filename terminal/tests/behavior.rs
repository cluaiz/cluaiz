use cluaiz_terminal::{render, Action, App, Limits, Metadata, Theme};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};

fn app() -> App {
    App::new(
        Metadata {
            title: "UI fixture".into(),
            workspace: "workspace".into(),
            status: "No Engine connected".into(),
            ..Metadata::default()
        },
        Limits::default(),
    )
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn ctrl(character: char) -> Event {
    Event::Key(KeyEvent::new(
        KeyCode::Char(character),
        KeyModifiers::CONTROL,
    ))
}

fn frame(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    terminal
        .draw(|frame| render(frame, app))
        .expect("render frame");
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
fn test_paste_is_text_without_implicit_submission() {
    let mut app = app();
    let text = "/theme\n!command\nनमस्ते";
    assert_eq!(app.handle(Event::Paste(text.into())), Action::None);
    assert_eq!(app.draft(), text);
    assert!(!app.theme_picker_open());
}

#[test]
fn test_large_paste_retains_all_text() {
    let mut app = app();
    let text = "x".repeat(100 * 1024);
    app.handle(Event::Paste(text.clone()));
    assert_eq!(app.draft(), text);
    assert_eq!(app.handle(key(KeyCode::Enter)), Action::Submit(text));
    assert_eq!(app.draft(), "");
}

#[test]
fn test_paste_over_limit_preserves_existing_draft() {
    let mut app = App::new(
        Metadata::default(),
        Limits {
            input_bytes: 5,
            ..Limits::default()
        },
    );
    app.handle(Event::Paste("kept".into()));
    app.handle(Event::Paste("too large".into()));
    assert_eq!(app.draft(), "kept");
    assert!(app
        .notice
        .as_deref()
        .expect("limit notice")
        .contains("unchanged"));
}

#[test]
fn test_multibyte_input_limit_is_bytes_not_characters() {
    let mut app = App::new(
        Metadata::default(),
        Limits {
            input_bytes: 4,
            ..Limits::default()
        },
    );
    app.handle(Event::Paste("界".into()));
    app.handle(key(KeyCode::Char('界')));
    assert_eq!(app.draft(), "界");
}

#[test]
fn test_portable_newline_and_submission_preserve_whitespace() {
    let mut app = app();
    app.handle(Event::Paste(" one ".into()));
    assert_eq!(app.handle(ctrl('j')), Action::None);
    app.handle(Event::Paste(" two ".into()));
    assert_eq!(
        app.handle(key(KeyCode::Enter)),
        Action::Submit(" one \n two ".into())
    );
}

#[test]
fn test_empty_input_does_not_submit() {
    let mut app = app();
    app.handle(Event::Paste("  \n ".into()));
    assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
    assert_eq!(app.draft(), "  \n ");
}

#[test]
fn test_key_release_and_repeat_do_not_submit() {
    let mut app = app();
    app.handle(Event::Paste("draft".into()));
    for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
        let event = Event::Key(KeyEvent {
            kind,
            ..KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        });
        assert_eq!(app.handle(event), Action::None);
        assert_eq!(app.draft(), "draft");
    }
}

#[test]
fn test_theme_cancel_restores_original_and_draft() {
    let mut app = app();
    app.handle(Event::Paste("draft".into()));
    app.handle(key(KeyCode::F(2)));
    app.handle(key(KeyCode::Down));
    assert_eq!(app.theme, Theme::Dark);
    app.handle(key(KeyCode::Esc));
    assert_eq!(app.theme, Theme::Auto);
    assert!(!app.theme_picker_open());
    assert_eq!(app.draft(), "draft");
}

#[test]
fn test_theme_apply_and_second_open_use_current_selection() {
    let mut app = app();
    app.handle(key(KeyCode::F(2)));
    app.handle(key(KeyCode::Down));
    app.handle(key(KeyCode::Down));
    app.handle(key(KeyCode::Enter));
    assert_eq!(app.theme, Theme::Light);
    app.handle(key(KeyCode::F(2)));
    app.handle(key(KeyCode::Esc));
    assert_eq!(app.theme, Theme::Light);
}

#[test]
fn test_overlay_owns_paste_and_escape_focus() {
    let mut app = app();
    app.handle(key(KeyCode::F(2)));
    app.handle(Event::Paste("hidden edit".into()));
    assert!(app.draft().is_empty());
    assert_eq!(app.handle(key(KeyCode::Esc)), Action::None);
    assert_eq!(app.handle(key(KeyCode::Esc)), Action::Cancel);
    assert_eq!(app.handle(ctrl('q')), Action::Exit);
}

#[test]
fn test_editor_navigation_and_undo_use_widget() {
    let mut app = app();
    app.handle(Event::Paste("ac".into()));
    app.handle(key(KeyCode::Left));
    app.handle(key(KeyCode::Char('b')));
    assert_eq!(app.draft(), "abc");
    app.handle(ctrl('z'));
    assert_eq!(app.draft(), "ac");
    app.handle(ctrl('y'));
    assert_eq!(app.draft(), "abc");
}

#[test]
fn test_all_themes_render_across_resize_without_losing_draft() {
    let mut app = app();
    app.handle(Event::Paste("Draft नमस्ते 界 e\u{301} 👩‍💻".into()));
    let draft = app.draft();
    for theme in Theme::ALL {
        app.theme = theme;
        for (width, height) in [(40, 12), (80, 24), (120, 40), (40, 12)] {
            let screen = frame(&mut app, width, height);
            assert!(screen.contains("UI fixture"));
            assert!(screen.contains("Ctrl+Q"));
            assert!(screen.contains("Draft"));
            assert!(screen.contains("नमस्ते"));
            assert_eq!(app.draft(), draft);
        }
    }
}

#[test]
fn test_question_help_is_contextual_and_paste_stays_literal() {
    let mut app = app();
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::Char('?'),
        KeyModifiers::SHIFT,
    )));
    assert!(frame(&mut app, 80, 24).contains("Shortcuts"));
    app.handle(Event::Key(KeyEvent {
        kind: KeyEventKind::Repeat,
        ..KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)
    }));
    assert!(frame(&mut app, 80, 24).contains("Shortcuts"));
    assert!(app.draft().is_empty());
    app.handle(key(KeyCode::Esc));
    assert!(!frame(&mut app, 80, 24).contains("Shortcuts"));
    app.set_draft("why").unwrap();
    app.handle(key(KeyCode::Char('?')));
    assert_eq!(app.draft(), "why?");
    app.set_draft("").unwrap();
    app.handle(Event::Paste("?".into()));
    assert_eq!(app.draft(), "?");
    assert!(!frame(&mut app, 80, 24).contains("Shortcuts"));
}

#[test]
fn test_help_scrolls_in_small_windows_and_never_submits() {
    let mut app = app();
    app.handle(key(KeyCode::Char('?')));
    let mut contents = frame(&mut app, 40, 12);
    for _ in 0..8 {
        assert_eq!(app.handle(key(KeyCode::PageDown)), Action::None);
        contents.push_str(&frame(&mut app, 40, 12));
    }
    assert!(contents.contains("Send your message"));
    assert!(contents.contains("Exit and return to your shell"));
    assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
    assert_eq!(app.handle(ctrl('q')), Action::Exit);
}

#[test]
fn test_thinking_collapses_without_losing_source_or_answer() {
    use cluaiz_terminal::transcript::Role;
    let mut app = app();
    app.transcript
        .upsert_role("u", Role::User, "You", "question")
        .unwrap();
    app.transcript
        .upsert_role("t", Role::Thinking, "Thinking", "private reasoning नमस्ते")
        .unwrap();
    app.transcript
        .upsert_role("a", Role::Assistant, "Cluaiz", "final answer")
        .unwrap();
    let collapsed = frame(&mut app, 80, 24);
    assert!(collapsed.contains("> You"));
    assert!(collapsed.contains("● Cluaiz"));
    assert!(collapsed.contains("Ctrl+O: expand"));
    assert!(!collapsed.contains("private reasoning"));
    app.set_draft("unfinished question").unwrap();
    app.handle(ctrl('o'));
    let expanded = frame(&mut app, 80, 24);
    assert!(expanded.contains("private reasoning नमस्ते"));
    assert!(expanded.contains("final answer"));
    assert_eq!(app.draft(), "unfinished question");
    app.handle(ctrl('o'));
    assert!(!frame(&mut app, 80, 24).contains("private reasoning"));
}

#[test]
fn test_role_styles_follow_theme_without_label_detection() {
    use cluaiz_terminal::transcript::Role;
    let mut app = app();
    // Identical labels must not decide which style or marker an entry receives.
    app.transcript
        .upsert_role("u", Role::User, "same", "user")
        .unwrap();
    app.transcript
        .upsert_role("a", Role::Assistant, "same", "answer")
        .unwrap();
    for theme in Theme::ALL {
        let rows = app.transcript.visible_styled(80, 24, theme.palette());
        let user = rows.iter().find(|row| row.to_string() == "user").unwrap();
        let answer = rows.iter().find(|row| row.to_string() == "answer").unwrap();
        assert_eq!(user.style, theme.palette().accent);
        assert_eq!(answer.style, theme.palette().base);
        assert!(rows.iter().any(|row| row.to_string() == "> same"));
        assert!(rows.iter().any(|row| row.to_string() == "● same"));
    }
}

#[test]
fn test_brand_falls_back_without_cropping_or_losing_draft() {
    let mut app = app();
    app.metadata.brand = "FULL ART\nSECOND ROW\nTHIRD ROW";
    app.metadata.compact_brand = "SMALL ART\nSECOND ROW";
    app.set_draft("draft kept").unwrap();
    assert!(frame(&mut app, 100, 24).contains("THIRD ROW"));
    let narrow = frame(&mut app, 24, 9);
    assert!(!narrow.contains("FULL ART"));
    assert!(narrow.contains("draft kept"));
    app.metadata.brand = "too wide";
    app.metadata.compact_brand = "pet";
    assert!(frame(&mut app, 46, 24).contains("pet"));
    assert_eq!(app.draft(), "draft kept");
}

#[test]
fn test_brand_header_wraps_long_workspace_and_status() {
    let mut app = app();
    app.metadata.brand = "pet\npet\npet\npet\npet\npet\npet\npet";
    app.metadata.workspace = format!("{} workspace-end", "folder/".repeat(13));
    app.metadata.status = format!("{} status-end", "context ".repeat(11));
    let view = frame(&mut app, 80, 24);
    assert!(view.contains("workspace-end"));
    assert!(view.contains("status-end"));
}

#[test]
fn test_tiny_and_zero_viewports_are_recoverable() {
    let mut app = app();
    for size in [(0, 0), (1, 1), (10, 4), (24, 9)] {
        frame(&mut app, size.0, size.1);
    }
    assert!(frame(&mut app, 80, 24).contains("UI fixture"));
}

#[test]
fn test_picker_remains_inside_small_viewport() {
    let mut app = app();
    app.handle(key(KeyCode::F(2)));
    for (width, height) in [(24, 9), (40, 12), (80, 24)] {
        let screen = frame(&mut app, width, height);
        for theme in Theme::ALL {
            assert!(screen.contains(theme.name()));
        }
        assert!(screen.contains("Esc: cancel"));
    }
}

#[test]
fn test_control_sequences_cannot_reach_rendered_cells() {
    let mut app = app();
    let hostile = "before\u{1b}[2J\u{1b}]52;c;payload\u{7}after";
    app.transcript
        .upsert("id", "Untrusted", hostile)
        .expect("entry");
    let screen = frame(&mut app, 80, 24);
    assert!(!screen.contains('\u{1b}'));
    assert!(!screen.contains('\u{7}'));
    assert!(screen.contains("before"));
    assert!(screen.contains("after"));
}

#[test]
fn test_stable_id_replaces_display_entry_without_duplicate() {
    let mut app = app();
    app.transcript
        .upsert("id", "Assistant", "old text")
        .expect("entry");
    app.transcript
        .upsert("id", "Assistant", "new text")
        .expect("update");
    let screen = frame(&mut app, 80, 24);
    assert_eq!(screen.matches("Assistant").count(), 1);
    assert!(!screen.contains("old text"));
    assert!(screen.contains("new text"));
}

#[test]
fn test_reading_older_output_does_not_jump_on_append() {
    let mut app = app();
    for index in 0..30 {
        app.transcript
            .upsert(&index.to_string(), "Entry", &format!("Body {index}"))
            .expect("entry");
    }
    frame(&mut app, 80, 24);
    app.handle(key(KeyCode::PageUp));
    let before = app.transcript.visible(80, 15);
    app.transcript
        .upsert("next", "New", "New output")
        .expect("entry");
    let after = app.transcript.visible(80, 15);
    assert_eq!(before, after);
    assert!(!app.transcript.following());
    app.handle(Event::Key(KeyEvent::new(
        KeyCode::End,
        KeyModifiers::CONTROL,
    )));
    assert!(frame(&mut app, 80, 24).contains("New output"));
}

#[test]
fn test_resize_anchor_keeps_the_same_logical_line() {
    let mut app = app();
    let content = (0..20)
        .map(|index| format!("logical-{index}: {}", "text ".repeat(24)))
        .collect::<Vec<_>>()
        .join("\n");
    app.transcript
        .upsert("id", "Entry", &content)
        .expect("entry");
    app.transcript.visible(80, 6);
    app.transcript.scroll(-1000);
    app.transcript.scroll(15);
    let before = app.transcript.visible(80, 6);
    let after = app.transcript.visible(40, 6);
    assert!(before[0].to_string().starts_with("logical-7:"));
    assert!(after[0].to_string().starts_with("logical-7:"));
    assert!(!app.transcript.following());
}

#[test]
fn test_retention_and_entry_limits_are_explicit() {
    let limits = Limits {
        retained_entries: 1,
        entry_bytes: 32,
        ..Limits::default()
    };
    let mut app = App::new(Metadata::default(), limits);
    app.transcript
        .upsert("first", "Entry", "old")
        .expect("entry");
    app.transcript
        .upsert("second", "Entry", "new")
        .expect("entry");
    assert_eq!(app.transcript.omitted_entries(), 1);
    assert!(frame(&mut app, 100, 24).contains("older display entries evicted"));
    assert!(app
        .transcript
        .upsert("huge", "Entry", &"x".repeat(33))
        .is_err());
}
