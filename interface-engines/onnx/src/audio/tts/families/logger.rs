use std::sync::Mutex;
use std::io::Write;
use std::sync::OnceLock;

static LOG_FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

fn get_log_file() -> &'static Mutex<Option<std::fs::File>> {
    LOG_FILE.get_or_init(|| Mutex::new(None))
}

/// Reset and initialize `log.txt` at the start of a fresh run
pub fn init_new_run_log() {
    let mutex = get_log_file();
    if let Ok(mut guard) = mutex.lock() {
        if let Ok(file) = std::fs::File::create("log.txt") {
            *guard = Some(file);
        }
    }
}

/// Log detailed point-by-point execution step into `log.txt`
pub fn log_step(family: &str, stage: &str, details: &str) {
    let mutex = get_log_file();
    let mut guard = mutex.lock().unwrap();
    if guard.is_none() {
        if let Ok(file) = std::fs::File::create("log.txt") {
            *guard = Some(file);
        }
    }
    
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() % 86400;
    let hours = (secs / 3600 + 5) % 24; // IST Offset approximation
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    let ms = duration.subsec_millis();
    
    let log_entry = format!(
        "[{:02}:{:02}:{:02}.{:03}] [{:<10}] [{:<25}] {}\n",
        hours, mins, s, ms, family, stage, details
    );
    
    // Output to stderr for active terminal inspection
    eprint!("{}", log_entry);
    
    // Append to log.txt
    if let Some(ref mut file) = *guard {
        let _ = file.write_all(log_entry.as_bytes());
        let _ = file.flush();
    }
}
