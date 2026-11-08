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

#[tokio::test(flavor = "multi_thread")]
async fn always_allow_saves_the_rule_for_the_folder_and_stops_asking() {
    use std::sync::{mpsc, Arc};
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let (sender, receiver) = mpsc::sync_channel(64);
    let mut tools = Tools::new(dir.path().into(), false);
    tools.set_display(Display::screen(sender, Arc::default()));
    let responder = std::thread::spawn(move || {
        let mut asked = Vec::new();
        let mut ended = 0;
        while let Ok(event) = receiver.recv() {
            match event {
                Event::Approval { rule, reply, .. } => {
                    asked.push(rule.clone());
                    reply.send(ApprovalResponse::Remember).unwrap();
                }
                Event::ToolEnd { .. } => {
                    ended += 1;
                    if ended == 2 {
                        break;
                    }
                }
                _ => {}
            }
        }
        asked
    });
    let first = tools.run("write_file", r#"{"path": "notes.txt", "content": "one"}"#).await;
    assert!(!first.starts_with("Error:"), "{first}");
    // The same file again: the saved rule allows it without asking
    let second = tools.run("write_file", r#"{"path": "notes.txt", "content": "two"}"#).await;
    assert!(!second.starts_with("Error:"), "{second}");
    let asked = responder.join().unwrap();
    assert_eq!(asked, [Some("write_file(notes.txt)".to_string())]);
    let saved = std::fs::read_to_string(dir.path().join(".agents/settings.local.json")).unwrap();
    assert!(saved.contains("write_file(notes.txt)"), "{saved}");
    // Kept out of git in this clone only
    let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|line| line == "/.agents/settings.local.json"), "{exclude}");
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
    assert!(!cluaiz_agents::ExecutionPolicy::chains("git status"));
    assert!(!cluaiz_agents::ExecutionPolicy::chains("Get-ChildItem src"));
    for line in [
        "ls; rm -r src",
        "echo hi > notes.txt",
        "cat a | tee b",
        "dir && del x",
        "echo $(whoami)",
    ] {
        assert!(cluaiz_agents::ExecutionPolicy::chains(line), "{line}");
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

#[test]
fn a_call_row_shows_every_argument() {
    let call = |name: &str, args: &str| describe_call(name, &serde_json::from_str(args).unwrap());
    assert_eq!(call("search", r#"{"pattern":"fn main","path":"src","glob":"*.rs"}"#), "\"fn main\" in src · files *.rs");
    assert_eq!(call("search", r#"{"pattern":"todo"}"#), "\"todo\" in .");
    assert_eq!(call("read_file", r#"{"path":"a.rs","offset":40,"limit":20}"#), "a.rs · lines from 40, at most 20");
    assert_eq!(call("write_file", r#"{"path":"hi.txt","content":"a\nb"}"#), "hi.txt · 2 lines");
    assert_eq!(call("run_command", r#"{"command":"cargo test","timeout":300}"#), "cargo test · timeout 300s");
}

#[tokio::test(flavor = "multi_thread")]
async fn remember_asks_even_in_full_access_and_saves_only_a_confirmed_fact() {
    use std::sync::{mpsc, Arc};
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let (sender, receiver) = mpsc::sync_channel(64);
    // Full Access: everything else runs without asking
    let mut tools = Tools::new(dir.path().into(), true);
    tools.set_display(Display::screen(sender, Arc::default()));
    let responder = std::thread::spawn(move || {
        let mut asked = Vec::new();
        let mut answers = vec![ApprovalResponse::Deny, ApprovalResponse::Allow].into_iter();
        let mut ended = 0;
        while let Ok(event) = receiver.recv() {
            match event {
                Event::Approval { reason, rule, reply, .. } => {
                    asked.push((reason, rule));
                    reply.send(answers.next().unwrap_or(ApprovalResponse::Deny)).unwrap();
                }
                Event::ToolEnd { .. } => {
                    ended += 1;
                    if ended == 3 {
                        break;
                    }
                }
                _ => {}
            }
        }
        asked
    });
    let call = r#"{"name": "Test command", "fact": "Run the tests with cargo test --release -p cmd."}"#;
    let refused = tools.run("remember", call).await;
    assert!(refused.starts_with("Error:"), "{refused}");
    let file = dir.path().join(".agents/memory/test-command.md");
    assert!(!file.exists(), "a refused fact is not written");
    let saved = tools.run("remember", call).await;
    assert!(saved.contains("test-command.md"), "{saved}");
    // read_item finds it in the same session
    assert!(tools.run("read_item", r#"{"name": "test-command"}"#).await.contains("cargo test --release -p cmd"));
    let asked = responder.join().unwrap();
    assert_eq!(asked.len(), 2);
    assert!(asked[0].0.contains("cargo test --release -p cmd") && asked[0].0.contains("test-command.md"), "{}", asked[0].0);
    // Never offered as a rule to always allow
    assert!(asked.iter().all(|(_, rule)| rule.is_none()));
    assert!(std::fs::read_to_string(&file).unwrap().contains("status: confirmed"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_glob_rule_comes_with_the_first_result_of_a_matching_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join(".agents/rules")).unwrap();
    std::fs::create_dir_all(root.join("web")).unwrap();
    std::fs::write(root.join(".agents/rules/typescript.md"), "---\ntrigger: glob\nglobs: \"*.ts\"\n---\nUse strict TypeScript; no any.").unwrap();
    std::fs::write(root.join("web/app.ts"), "let x = 1;\n").unwrap();
    std::fs::write(root.join("notes.md"), "notes\n").unwrap();
    let mut tools = Tools::new(root.into(), true);
    tools.rescan_workspace();
    let first = tools.run("read_file", r#"{"path": "web/app.ts"}"#).await;
    assert!(first.contains("let x = 1;") && first.contains("The project's rule typescript applies to web/app.ts") && first.contains("no any"), "{first}");
    // Once in a conversation
    let again = tools.run("edit_file", r#"{"path": "web/app.ts", "old_text": "let x = 1;", "new_text": "const x = 1;"}"#).await;
    assert!(!again.contains("no any"), "{again}");
    assert!(!tools.run("read_file", r#"{"path": "notes.md"}"#).await.contains("applies to"));
    assert!(tools.sent.contains("rule:typescript"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_third_identical_call_in_a_request_says_so() {
    let (mut t, _dir) = tools();
    let call = r#"{"path": "src/main.rs"}"#;
    assert!(!t.run("read_file", call).await.contains("same call"));
    assert!(!t.run("read_file", call).await.contains("same call"));
    // Other arguments are another call
    assert!(!t.run("read_file", r#"{"path": "src/main.rs", "offset": 1}"#).await.contains("same call"));
    let third = t.run("read_file", call).await;
    assert!(third.starts_with("    1\tfn main() {") && third.contains("made 3 times"), "{third}");
    // A new request counts again
    t.calls.clear();
    assert!(!t.run("read_file", call).await.contains("same call"));
}
