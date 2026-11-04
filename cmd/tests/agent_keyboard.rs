//! Opt-in: the real agent screen in a terminal, with a `!command` that asks a question and the
//! answer typed on its keyboard. Needs an isolated home with a chat model.
//! `CLUAIZ_KEYBOARD_HOME=<home> cargo test -p cmd --test agent_keyboard -- --ignored --nocapture`

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The screen's output so far, without escape sequences and line ends
fn seen(raw: &Arc<Mutex<Vec<u8>>>) -> String {
    let bytes = raw.lock().unwrap().clone();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.next() == Some('[') {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
        } else if !c.is_control() {
            out.push(c);
        }
    }
    out
}

fn wait_for(raw: &Arc<Mutex<Vec<u8>>>, text: &str, seconds: u64) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(seconds) {
        if seen(raw).contains(text) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let shown = seen(raw);
    let tail: String = shown.chars().rev().take(1500).collect::<Vec<_>>().into_iter().rev().collect();
    panic!("{text:?} did not show in {seconds} s; the end of the screen: {tail}");
}

#[test]
#[ignore = "starts the real agent; needs CLUAIZ_KEYBOARD_HOME"]
fn a_user_command_is_answered_on_the_keyboard() {
    let home = std::env::var("CLUAIZ_KEYBOARD_HOME").expect("CLUAIZ_KEYBOARD_HOME");
    let pair = native_pty_system().openpty(PtySize { rows: 40, cols: 140, pixel_width: 0, pixel_height: 0 }).unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_cluaiz"));
    command.args(["agent", "--yes"]);
    if let Ok(model) = std::env::var("CLUAIZ_KEYBOARD_MODEL") {
        command.args(["--model", &model]);
    }
    command.cwd(std::env::temp_dir());
    command.env("CLUAIZ_HOME", &home);
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let raw: Arc<Mutex<Vec<u8>>> = Arc::default();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
    let (kept, replies) = (raw.clone(), writer.clone());
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            // The console and the screen library ask where the cursor is
            if buffer[..count].windows(4).any(|w| w == b"\x1b[6n") {
                let mut writer = replies.lock().unwrap();
                let _ = writer.write_all(b"\x1b[1;1R").and_then(|_| writer.flush());
            }
            kept.lock().unwrap().extend_from_slice(&buffer[..count]);
        }
    });
    let typed = |text: &str| {
        let mut writer = writer.lock().unwrap();
        for c in text.chars() {
            let mut one = [0u8; 4];
            writer.write_all(c.encode_utf8(&mut one).as_bytes()).unwrap();
            writer.flush().unwrap();
            std::thread::sleep(Duration::from_millis(15));
        }
    };

    // The screen is up when its input box takes keys; the model may still be loading
    std::thread::sleep(Duration::from_secs(8));
    let ask = if cfg!(windows) {
        "!$n = Read-Host 'Your name'; $d = Read-Host 'Default'; Write-Output \"Hello-$n-[$d]\""
    } else {
        "!printf 'Your name: '; read n; printf 'Default: '; read d; echo \"Hello-$n-[$d]\""
    };
    typed(ask);
    typed("\r");
    wait_for(&raw, "keyboard", 60);
    wait_for(&raw, "Your name", 60);
    typed("Asha");
    typed("\r");
    wait_for(&raw, "Default", 30);
    // Enter alone: an empty answer
    typed("\r");
    wait_for(&raw, "Hello-Asha-[]", 30);
    println!("the command took both answers");

    // The model's own command, when it marks it as interactive (a small model may not)
    if std::env::var("CLUAIZ_KEYBOARD_ASK_MODEL").is_ok() {
        // Neither the question nor the result is in the message itself
        let question = if cfg!(windows) { "Write-Output ('Got' + '-' + (Read-Host ('Your' + ' city')))" } else { "printf 'Your'; printf ' city: '; read c; echo Got''-$c" };
        typed(&format!("Run this with run_command and interactive set to true, because I answer it at the keyboard: {question}\r"));
        wait_for(&raw, "Your city", 240);
        typed("Pune");
        typed("\r");
        wait_for(&raw, "Got-Pune", 60);
        println!("the model's command took the answer");
    }

    let _ = child.kill();
}
