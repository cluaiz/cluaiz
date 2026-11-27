//! Records why llama.cpp / ggml aborted the process, so the next start can say so.
//!
//! `ggml_abort` (`ggml.c`) prints, calls the callback set by `ggml_set_abort_callback`, then always calls
//! `abort()`. The callback cannot stop that. It only writes the message to stderr and appends it to
//! `<home>/last_crash.txt`; the next `serve` start reports the file and renames it to `last_crash.prev.txt`.

use std::io::Write;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MARKER: &str = "last_crash.txt";
const PREVIOUS: &str = "last_crash.prev.txt";
/// The message is cut here so a huge string cannot grow the file.
const MAX_MESSAGE_BYTES: usize = 8 * 1024;

static MARKER_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn marker_path(home: &Path) -> PathBuf {
    home.join(MARKER)
}

/// One line: unix seconds, a tab, the message with line breaks turned into ` | `.
pub fn format_entry(unix_seconds: u64, message: &str) -> String {
    let mut text = message.trim().replace("\r\n", " | ").replace('\n', " | ");
    if text.len() > MAX_MESSAGE_BYTES {
        let mut cut = MAX_MESSAGE_BYTES;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
    format!("{unix_seconds}\t{text}\n")
}

/// Appends one entry and flushes it to the disk.
pub fn write_marker(path: &Path, unix_seconds: u64, message: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(format_entry(unix_seconds, message).as_bytes())?;
    file.sync_all()
}

/// Returns the content of the marker (trimmed) and renames it to `last_crash.prev.txt`
/// (an older `.prev` is replaced). `None` when there is no marker.
pub fn take_marker(home: &Path) -> Option<String> {
    let path = marker_path(home);
    let content = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(home.join(PREVIOUS));
    if std::fs::rename(&path, home.join(PREVIOUS)).is_err() {
        // If it cannot be renamed it would be reported at every start; remove it only as a last resort.
        let _ = std::fs::remove_file(&path);
    }
    let content = content.trim().to_string();
    if content.is_empty() { None } else { Some(content) }
}

extern "C" fn on_abort(message: *const c_char) {
    // No panic may cross this boundary and the process dies right after; errors are ignored.
    let text = if message.is_null() {
        "ggml_abort without a message".to_string()
    } else {
        unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy().into_owned()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = writeln!(std::io::stderr(), "[cluaiz] llama.cpp aborted: {}", text.trim());
    if let Some(path) = MARKER_PATH.get() {
        let _ = write_marker(path, now, &text);
    }
}

/// Installs the callback once. `home` is the Cluaiz home folder. Call after the home folder exists.
pub fn install(home: &Path) {
    if MARKER_PATH.set(marker_path(home)).is_ok() {
        unsafe { crate::ffi::llama_cpp::ggml_set_abort_callback(Some(on_abort)) };
    }
}

/// At a `serve` start: warns once when the last run ended in a llama.cpp abort.
pub fn report_previous(home: &Path) {
    if let Some(content) = take_marker(home) {
        tracing::warn!(
            "the last run ended in a llama.cpp abort: {content} (kept in {})",
            home.join(PREVIOUS).display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_take_renames_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        write_marker(&marker_path(dir.path()), 42, "GGML_ASSERT(x) failed\nCUDA error").unwrap();
        let got = take_marker(dir.path()).unwrap();
        assert_eq!(got, "42\tGGML_ASSERT(x) failed | CUDA error");
        assert!(!marker_path(dir.path()).exists());
        assert!(dir.path().join(PREVIOUS).exists());
        assert!(take_marker(dir.path()).is_none());
    }

    #[test]
    fn second_crash_replaces_older_prev() {
        let dir = tempfile::tempdir().unwrap();
        write_marker(&marker_path(dir.path()), 1, "first").unwrap();
        take_marker(dir.path());
        write_marker(&marker_path(dir.path()), 2, "second").unwrap();
        assert_eq!(take_marker(dir.path()).unwrap(), "2\tsecond");
        assert!(std::fs::read_to_string(dir.path().join(PREVIOUS)).unwrap().contains("second"));
    }

    #[test]
    fn long_message_is_cut_on_a_char_boundary() {
        let long = "é".repeat(MAX_MESSAGE_BYTES);
        let entry = format_entry(1, &long);
        assert!(entry.len() <= MAX_MESSAGE_BYTES + 16);
        assert!(entry.ends_with('\n'));
    }
}
