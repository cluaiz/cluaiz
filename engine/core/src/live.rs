//! What the running generation is doing, for a terminal that shows progress while nothing is printed
//! (the CLI): tokens generated so far, and the tool call being written (its name and arguments so
//! far; the call itself is only sent when generation ends).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static TOKENS: AtomicUsize = AtomicUsize::new(0);
static CALL: Mutex<Option<(String, String)>> = Mutex::new(None);

/// A generation starts
pub fn start() {
    TOKENS.store(0, Ordering::Relaxed);
    if let Ok(mut call) = CALL.lock() {
        *call = None;
    }
}

/// One more token (thinking included)
pub fn token() {
    TOKENS.fetch_add(1, Ordering::Relaxed);
}

/// The call being written: its name and its arguments' text so far
pub fn set_call(name: &str, arguments: &str) {
    if let Ok(mut call) = CALL.lock() {
        *call = Some((name.to_string(), arguments.to_string()));
    }
}

pub fn tokens() -> usize {
    TOKENS.load(Ordering::Relaxed)
}

pub fn call() -> Option<(String, String)> {
    CALL.lock().ok().and_then(|call| call.clone())
}
