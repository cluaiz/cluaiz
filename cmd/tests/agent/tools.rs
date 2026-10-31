use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn strict_read_requires_permission_and_denial_does_not_expose_content() {
    use std::sync::{mpsc, Arc};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("read.txt"), "private test marker").unwrap();
    let (sender, receiver) = mpsc::sync_channel(16);
    let mut tools = Tools::new(dir.path().into(), false);
    tools.strict = true;
    tools.set_display(Display::screen(sender, Arc::default()));
    let responder = std::thread::spawn(move || {
        let mut asked = false;
        while let Ok(event) = receiver.recv() {
            if let Event::Approval { reason, reply, .. } = event {
                asked = true;
                assert!(reason.contains("read.txt"));
                reply.send(ApprovalResponse::Deny).unwrap();
            } else if matches!(event, Event::ToolEnd { .. }) {
                break;
            }
        }
        assert!(asked);
    });
    let result = tools.run("read_file", r#"{"path":"read.txt"}"#).await;
    assert!(result.starts_with("Error:"));
    assert!(!result.contains("private test marker"));
    responder.join().unwrap();
    tools.require_approval();
    assert!(!tools.strict);
}

fn tools() -> (Tools, PathBuf) {
    let dir = std::env::temp_dir().join(format!("cluaiz-agent-tools-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/main.rs"),
        "fn main() {\r\n    println!(\"hi\");\r\n}\r\n",
    )
    .unwrap();
    (Tools::new(dir.clone(), true), dir)
}

#[tokio::test(flavor = "multi_thread")]
async fn files_are_read_searched_and_edited() {
    let (mut t, dir) = tools();
    let read = t.run("read_file", r#"{"path": "src/main.rs"}"#).await;
    assert!(read.starts_with("    1\tfn main() {"), "{read}");
    let found = t
        .run("search", r#"{"pattern": "println", "glob": "*.rs"}"#)
        .await;
    assert_eq!(found, "src/main.rs:2: println!(\"hi\");");
    // Plain line breaks match the file's Windows ones
    let edit = t.run("edit_file", r#"{"path": "src/main.rs", "old_text": "{\n    println!(\"hi\");", "new_text": "{\n    println!(\"bye\");"}"#).await;
    assert_eq!(edit, "src/main.rs edited (1 replaced).");
    assert!(std::fs::read_to_string(dir.join("src/main.rs"))
        .unwrap()
        .contains("println!(\"bye\");\r\n"));
    let twice = t
        .run(
            "edit_file",
            r#"{"path": "src/main.rs", "old_text": "nothing like this", "new_text": "x"}"#,
        )
        .await;
    assert!(twice.starts_with("Error: old_text is not in the file"));
    assert_eq!(t.run("list_dir", "{}").await, "src/");
    assert!(t
        .run("nope", "{}")
        .await
        .starts_with("Error: there is no tool named 'nope'"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_read_only_start_does_not_cover_the_rest_of_the_line() {
    assert!(!chains("git status"));
    assert!(!chains("Get-ChildItem src"));
    for line in [
        "ls; rm -r src",
        "echo hi > notes.txt",
        "cat a | tee b",
        "dir && del x",
        "echo $(whoami)",
    ] {
        assert!(chains(line), "{line}");
    }
}

#[test]
fn paths_resolve_inside_the_folder() {
    let dir = std::env::temp_dir().join(format!("cluaiz-paths-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let t = Tools::new(dir.clone(), false);
    assert_eq!(t.resolve("src/../lib.rs").unwrap(), t.root.join("lib.rs"));
    assert!(!t.resolve("../outside").unwrap().starts_with(&t.root));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_denied_write_and_feedback_do_not_mutate_files() {
    use std::sync::{atomic::AtomicBool, mpsc, Arc};
    let dir = tempfile::tempdir().unwrap();
    for response in [
        ApprovalResponse::Deny,
        ApprovalResponse::Feedback("Keep My API".into()),
    ] {
        let (sender, receiver) = mpsc::sync_channel(16);
        let mut t = Tools::new(dir.path().into(), false);
        t.set_display(Display::screen(sender, Arc::new(AtomicBool::new(false))));
        let responder = std::thread::spawn(move || {
            let mut response = Some(response);
            while let Ok(event) = receiver.recv() {
                if let Event::Approval { reason, reply, .. } = event {
                    assert!(reason.contains("new.txt"));
                    assert!(reason.contains("+ proposed content"));
                    reply.send(response.take().unwrap()).unwrap();
                    // Keep receiving ToolEnd so the bounded channel stays connected.
                } else if matches!(event, Event::ToolEnd { .. }) {
                    break;
                }
            }
        });
        let result = t
            .run(
                "write_file",
                r#"{"path":"new.txt","content":"proposed content"}"#,
            )
            .await;
        assert!(result.starts_with("Error:"), "{result}");
        assert!(!dir.path().join("new.txt").exists());
        if result.contains("said:") {
            assert!(result.contains("Keep My API"));
        }
        responder.join().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_invalid_arguments_do_not_write_and_failed_command_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut t = Tools::new(dir.path().into(), true);
    for args in ["{", "[]", "null"] {
        assert!(t.run("write_file", args).await.starts_with("Error:"));
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    let result = t.run("run_command", r#"{"command":"exit 7"}"#).await;
    assert!(result.starts_with("Error:"), "{result}");
    assert!(result.contains("Exit code 7"), "{result}");
}
