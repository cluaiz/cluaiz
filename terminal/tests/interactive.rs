//! Developer-only preview: no production command and no simulated model/tool execution.

use cluaiz_terminal::{render, Action, App, Limits, Metadata, Screen};
use crossterm::{event, terminal::is_raw_mode_enabled};

#[test]
#[ignore = "requires an interactive terminal; Ctrl+Q exits the preview"]
fn test_interactive_preview_restores_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = Metadata {
        title: "Cluaiz | UI preview".into(),
        workspace: std::env::current_dir()?.display().to_string(),
        status: "P1 presentation preview | No Engine, model or tools connected".into(),
        ..Metadata::default()
    };
    let mut app = App::new(metadata, Limits::default());
    for index in 1..=24 {
        app.transcript.upsert(&format!("fixture-{index}"), &format!("UI fixture {index}"),
            "Sample transcript for scrolling and resize checks. नमस्ते | English | 界\nThis is test content, not model output.")?;
    }
    let mut screen = Screen::open()?;
    assert!(is_raw_mode_enabled()?);
    let mut sequence = 0;
    loop {
        screen.draw(|frame| render(frame, &mut app))?;
        match app.handle(event::read()?) {
            Action::Exit => break,
            Action::Submit(text) => {
                sequence += 1;
                app.transcript
                    .upsert(&format!("user-{sequence}"), "Your preview input", &text)?;
                app.notice =
                    Some("Input captured for this preview only; no AI or command executed".into());
            }
            Action::Cancel => app.notice = Some("No Engine attached; the draft was kept".into()),
            Action::Clipboard
            | Action::Permissions
            | Action::ThinkingMode
            | Action::RemoveAttachment(_)
            | Action::Choose { .. } => {
                app.notice =
                    Some("This developer preview has no client adapter; use cluaiz agent".into());
            }
            Action::Approval(_) => {}
            Action::None => {}
        }
    }
    screen.close()?;
    assert!(!is_raw_mode_enabled()?);
    println!("Terminal restored; preview executed no model or tools.");
    Ok(())
}

#[test]
#[ignore = "requires an interactive terminal; checks error and expected-panic cleanup"]
fn test_real_terminal_cleanup_after_error_and_panic() -> Result<(), Box<dyn std::error::Error>> {
    fn interrupted() -> std::io::Result<()> {
        let _screen = Screen::open()?;
        assert!(
            Screen::open().is_err(),
            "a second screen must not take terminal ownership"
        );
        Err(std::io::Error::other("test-only caller failure"))
    }
    assert!(interrupted().is_err());
    assert!(!is_raw_mode_enabled()?);
    let outcome = std::panic::catch_unwind(|| {
        let _screen = Screen::open().expect("real terminal");
        panic!("test-only expected panic to check restoration");
    });
    assert!(outcome.is_err());
    assert!(!is_raw_mode_enabled()?);
    Screen::open()?.close()?;
    println!("Terminal restored after caller error and expected panic; reopened successfully.");
    Ok(())
}
