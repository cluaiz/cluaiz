//! A command in a real terminal (ConPTY on Windows, a pty elsewhere) whose keyboard is the user's:
//! installers and prompts that ask questions, which a pipe or no input at all cannot answer. Only
//! for a command the user typed or one the model marks as interactive; everything else uses
//! [`SandboxTerminalRunner::execute_streaming`].

use super::environment::EnvironmentResolver;
use super::result::TerminalRunResult;
use super::terminal_runner::SandboxTerminalRunner;
use crate::registry::types::SecurityMode;
use anyhow::{anyhow, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc::UnboundedReceiver, mpsc::UnboundedSender, watch};

/// Wide enough that the terminal does not wrap a line of output
const COLUMNS: u16 = 200;
const ROWS: u16 = 50;
/// Text without a line end is shown after this long: it is usually a question
const PROMPT_AFTER: Duration = Duration::from_millis(200);
const POLL: Duration = Duration::from_millis(30);
/// How long the last output may take to arrive once the command has ended
const DRAIN: Duration = Duration::from_secs(2);
/// A cursor request that the terminal's owner has to answer (the Windows console sends one at start)
const CURSOR_REQUEST: &[u8] = b"\x1b[6n";
const CURSOR_REPLY: &[u8] = b"\x1b[1;1R";

/// Terminal output as plain text: escape sequences go, a bare carriage return starts its line over
/// (progress bars), and the cursor moves the Windows console uses in place of spaces become spaces.
#[derive(Default)]
struct Plain {
    state: Escape,
    params: Vec<u8>,
    carriage: bool,
    line: Vec<u8>,
}

#[derive(Default, PartialEq)]
enum Escape {
    #[default]
    None,
    Started,
    Csi,
    /// Until BEL or ESC \ (window titles and the like)
    Text,
    TextEsc,
}

impl Plain {
    /// Feeds bytes; finished lines go to `done`
    fn feed(&mut self, bytes: &[u8], done: &mut Vec<String>) {
        for &byte in bytes {
            match self.state {
                Escape::Started => {
                    self.state = match byte {
                        b'[' => {
                            self.params.clear();
                            Escape::Csi
                        }
                        b']' | b'P' | b'^' | b'_' => Escape::Text,
                        _ => Escape::None,
                    };
                    continue;
                }
                Escape::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.state = Escape::None;
                        if byte == b'C' {
                            let count: usize = std::str::from_utf8(&self.params).ok().and_then(|p| p.parse().ok()).unwrap_or(1);
                            self.line.extend(std::iter::repeat_n(b' ', count.min(COLUMNS as usize)));
                        }
                    } else {
                        self.params.push(byte);
                    }
                    continue;
                }
                Escape::Text => {
                    self.state = match byte {
                        0x07 => Escape::None,
                        0x1b => Escape::TextEsc,
                        _ => Escape::Text,
                    };
                    continue;
                }
                Escape::TextEsc => {
                    self.state = if byte == b'\\' { Escape::None } else { Escape::Text };
                    continue;
                }
                Escape::None => {}
            }
            if self.carriage && byte != b'\n' {
                self.line.clear();
            }
            self.carriage = false;
            match byte {
                0x1b => self.state = Escape::Started,
                b'\r' => self.carriage = true,
                b'\n' => done.push(self.take()),
                0x08 => {
                    self.line.pop();
                }
                b'\t' => self.line.push(byte),
                byte if byte < 0x20 || byte == 0x7f => {}
                byte => self.line.push(byte),
            }
        }
    }

    fn take(&mut self) -> String {
        let line = String::from_utf8_lossy(&self.line).trim_end().to_string();
        self.line.clear();
        line
    }

    fn partial(&self) -> String {
        String::from_utf8_lossy(&self.line).trim_end().to_string()
    }
}

/// Reads the terminal's output: the whole of it for the result, and each line and each question
/// (text without a line end that stays) for the live view
fn collect(chunks: mpsc::Receiver<Vec<u8>>, on_line: UnboundedSender<String>, output: Arc<Mutex<Vec<String>>>) {
    let mut plain = Plain::default();
    let mut shown = 0usize;
    loop {
        let received = chunks.recv_timeout(PROMPT_AFTER);
        let mut done = Vec::new();
        match &received {
            Ok(bytes) => plain.feed(bytes, &mut done),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let partial = plain.partial();
                if partial.len() > shown {
                    let _ = on_line.send(partial[shown..].trim().to_string());
                    shown = partial.len();
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => done.push(plain.take()),
        }
        for line in done {
            // The part already shown as a question is not shown again
            let rest = line.get(shown..).unwrap_or("").trim();
            if !rest.is_empty() {
                let _ = on_line.send(rest.to_string());
            }
            shown = 0;
            if let Ok(mut all) = output.lock() {
                all.push(line);
            }
        }
        if received.is_err() {
            return;
        }
    }
}

impl SandboxTerminalRunner {
    /// Runs `command_line` in a terminal. Each string from `input` is typed into it, followed by
    /// Enter. Output lines go to `on_line`; the result holds them all as stdout (a terminal has one
    /// output). Stops at the timeout or when `cancel_rx` turns true.
    pub async fn execute_interactive(
        command_line: &str,
        cwd: &Path,
        security_mode: SecurityMode,
        timeout_secs: u64,
        cancel_rx: watch::Receiver<bool>,
        on_line: UnboundedSender<String>,
        input: UnboundedReceiver<String>,
    ) -> Result<TerminalRunResult> {
        Self::validate_command(command_line, cwd, security_mode)?;
        let (command, cwd) = (command_line.trim().to_owned(), cwd.to_path_buf());
        tokio::task::spawn_blocking(move || run(&command, &cwd, security_mode, timeout_secs, cancel_rx, on_line, input))
            .await
            .map_err(|e| anyhow!("The terminal task ended: {e}"))?
    }
}

fn run(
    command: &str,
    cwd: &Path,
    security_mode: SecurityMode,
    timeout_secs: u64,
    cancel_rx: watch::Receiver<bool>,
    on_line: UnboundedSender<String>,
    mut input: UnboundedReceiver<String>,
) -> Result<TerminalRunResult> {
    let started = Instant::now();
    let scrub_secrets = matches!(security_mode, SecurityMode::Inherit | SecurityMode::RequireApproval);
    let env = EnvironmentResolver::resolve(cwd, None, scrub_secrets);

    let pair = native_pty_system()
        .openpty(PtySize { rows: ROWS, cols: COLUMNS, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| anyhow!("Could not open a terminal: {e}"))?;

    #[cfg(target_os = "windows")]
    let mut builder = {
        let mut builder = CommandBuilder::new("powershell.exe");
        builder.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command]);
        builder
    };
    #[cfg(not(target_os = "windows"))]
    let mut builder = {
        let mut builder = CommandBuilder::new("sh");
        builder.args(["-c", command]);
        builder
    };
    builder.cwd(&env.cwd);
    builder.env_clear();
    for (key, value) in &env.env_vars {
        builder.env(key, value);
    }
    // A pager would wait for keys the live view cannot show well
    builder.env("PAGER", "cat");
    builder.env("GIT_PAGER", "cat");

    let mut child = pair.slave.spawn_command(builder).map_err(|e| anyhow!("Could not start the command in a terminal: {e}"))?;
    drop(pair.slave);
    let mut killer = child.clone_killer();
    let mut reader = pair.master.try_clone_reader().map_err(|e| anyhow!("Could not read the terminal: {e}"))?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(|e| anyhow!("Could not write to the terminal: {e}"))?));

    let output: Arc<Mutex<Vec<String>>> = Arc::default();
    let (chunk_tx, chunk_rx) = mpsc::channel::<Vec<u8>>();
    let replies = writer.clone();
    std::thread::Builder::new().name("cluaiz-pty-read".into()).spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let bytes = &buffer[..count];
                    if bytes.windows(CURSOR_REQUEST.len()).any(|w| w == CURSOR_REQUEST) {
                        if let Ok(mut writer) = replies.lock() {
                            let _ = writer.write_all(CURSOR_REPLY).and_then(|_| writer.flush());
                        }
                    }
                    if chunk_tx.send(bytes.to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    })?;
    let collected = output.clone();
    let collector = std::thread::Builder::new().name("cluaiz-pty-lines".into()).spawn(move || collect(chunk_rx, on_line, collected))?;

    let deadline = started + Duration::from_secs(timeout_secs);
    let mut cancelled = false;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if *cancel_rx.borrow() && !cancelled {
            cancelled = true;
            let _ = killer.kill();
        } else if Instant::now() >= deadline && !timed_out {
            timed_out = true;
            tracing::warn!("⏱️ [TerminalRunner] Command exceeded timeout of {timeout_secs}s. Terminating...");
            let _ = killer.kill();
        }
        while let Ok(text) = input.try_recv() {
            if let Ok(mut writer) = writer.lock() {
                let _ = writer.write_all(text.as_bytes()).and_then(|_| writer.write_all(b"\r")).and_then(|_| writer.flush());
            }
        }
        std::thread::sleep(POLL);
    };
    // Closing the terminal ends the reader; on Windows it can stay blocked, so it is not waited for
    drop(writer);
    drop(pair.master);
    let drained = Instant::now();
    while !collector.is_finished() && drained.elapsed() < DRAIN {
        std::thread::sleep(POLL);
    }
    let text = output.lock().map(|lines| lines.join("\n")).unwrap_or_default();
    let exit_code = status.map_or(1, |s| i32::try_from(s.exit_code()).unwrap_or(1));
    let exit_code = if cancelled { -1 } else { exit_code };
    Ok(TerminalRunResult::new(exit_code, text, String::new(), started.elapsed().as_millis() as u64, cancelled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    fn plain(bytes: &[u8]) -> (Vec<String>, String) {
        let mut plain = Plain::default();
        let mut done = Vec::new();
        plain.feed(bytes, &mut done);
        (done, plain.partial())
    }

    #[test]
    fn terminal_output_becomes_plain_lines() {
        let (lines, rest) = plain(b"\x1b[2J\x1b[?25l\x1b]0;title\x07Hello\x1b[3Cworld\r\n\x1b[32mgreen\x1b[0m\r\n10%\r100%\r\nName: ");
        assert_eq!(lines, ["Hello   world", "green", "100%"]);
        assert_eq!(rest, "Name:");
        assert_eq!(plain(b"ab\x08c\r\n").0, ["ac"]);
        // A sequence cut in two chunks
        let mut p = Plain::default();
        let mut done = Vec::new();
        p.feed(b"a\x1b[3", &mut done);
        p.feed(b"2mb\n", &mut done);
        assert_eq!(done, ["ab"]);
    }

    /// (command that asks a name and prints it, what it prints)
    fn asking() -> &'static str {
        if cfg!(windows) {
            "$n = Read-Host 'Your name'; Write-Output \"Hello $n\"; exit 3"
        } else {
            "printf 'Your name: '; read n; echo \"Hello $n\"; exit 3"
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_command_asks_and_the_keyboard_answers() {
        let dir = std::env::temp_dir();
        let (_cancel, cancel_rx) = watch::channel(false);
        let (line_tx, mut lines) = unbounded_channel();
        let (keys, key_rx) = unbounded_channel();
        let run = tokio::spawn(SandboxTerminalRunner::execute_interactive(asking(), Box::leak(dir.into_boxed_path()), SecurityMode::AlwaysAllow, 600, cancel_rx, line_tx, key_rx));
        // The question shows before anything is typed
        let waited = tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(line) = lines.recv().await {
                if line.contains("Your name") {
                    return true;
                }
            }
            false
        });
        assert!(waited.await.unwrap_or(false), "the question did not show");
        keys.send("Asha".to_string()).unwrap();
        let result = run.await.unwrap().unwrap();
        assert!(result.stdout.contains("Hello Asha"), "{}", result.stdout);
        assert_eq!(result.exit_code, 3);
        assert!(!result.cancelled);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stopping_and_the_timeout_end_a_waiting_command() {
        let dir: &'static Path = Box::leak(std::env::temp_dir().into_boxed_path());
        let (cancel, cancel_rx) = watch::channel(false);
        let (line_tx, mut lines) = unbounded_channel();
        let (_keys, key_rx) = unbounded_channel();
        let run = tokio::spawn(SandboxTerminalRunner::execute_interactive(asking(), dir, SecurityMode::AlwaysAllow, 600, cancel_rx, line_tx, key_rx));
        // Once it waits for an answer
        while let Some(line) = lines.recv().await {
            if line.contains("Your name") {
                break;
            }
        }
        cancel.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(20), run).await.expect("cancel must end it").unwrap().unwrap();
        assert!(result.cancelled);
        assert_eq!(result.exit_code, -1);

        let (_cancel, cancel_rx) = watch::channel(false);
        let (line_tx, _lines) = unbounded_channel();
        let (_keys, key_rx) = unbounded_channel();
        let started = Instant::now();
        let result = SandboxTerminalRunner::execute_interactive(asking(), dir, SecurityMode::AlwaysAllow, 2, cancel_rx, line_tx, key_rx).await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(20));
        assert!(!result.cancelled);
        assert!(!result.stdout.contains("Hello"));
    }
}
