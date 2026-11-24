//! MTP speculative decoding: the model's own multi-token-prediction (NextN) layers guess the next
//! few tokens, the target checks them all in one decode, and every guess it agrees with is a token
//! gained without a decode of its own. The drafter is llama.cpp's (`common_speculative`, type
//! `draft-mtp`, as `llama-server --spec-type draft-mtp`), through `ffi/speculative_bridge.cpp`; the
//! target decode and the check stay in `stream.rs`, so the answer is the one the target would give
//! token by token (a guess is kept only when the target's sampler picks the same token).
//!
//! Experimental. Turned on by the setting `optimization.mtp` (on, or Auto through the runtime's load
//! rules) or forced by the `CLUAIZ_MTP` test override ([`env_override`]); its memory is not in the
//! load plan yet.

use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicU64, Ordering};

unsafe extern "C" {
    fn cluaiz_mtp_init(model: *mut c_void, ctx: *mut c_void, n_max: i32, p_min: f32, err: *mut c_char, err_len: usize) -> *mut c_void;
    fn cluaiz_mtp_free(mtp: *mut c_void);
    fn cluaiz_mtp_n_max(mtp: *const c_void) -> i32;
    fn cluaiz_mtp_process(mtp: *mut c_void, tokens: *const i32, pos: *const i32, output: *const i8, n_tokens: i32) -> bool;
    fn cluaiz_mtp_begin(mtp: *mut c_void, prompt: *const i32, n_prompt: i32);
    fn cluaiz_mtp_draft(
        mtp: *mut c_void,
        pos0: i32,
        id_last: i32,
        prompt: *const i32,
        n_prompt: i32,
        n_max: i32,
        out: *mut i32,
        cap: i32,
    ) -> i32;
    fn cluaiz_mtp_accept(mtp: *mut c_void, n_accepted: u16);
    fn cluaiz_mtp_seq_rm(mtp: *mut c_void, p0: i32);
}

/// Tokens drafted per step, llama-server's default (`--spec-draft-n-max 3`)
pub const DEFAULT_DRAFT_MAX: i32 = 3;

/// The `CLUAIZ_MTP` test override: `Some(true)` forces MTP on, `Some(false)` forces it off, `None`
/// leaves the load to the setting (`optimization.mtp`, Auto by default). An unknown value is logged
/// and ignored.
pub fn env_override() -> Option<bool> {
    let raw = std::env::var("CLUAIZ_MTP").ok()?;
    let parsed = parse_override(&raw);
    if parsed.is_none() && !raw.trim().is_empty() {
        tracing::warn!("🔮 [MTP] CLUAIZ_MTP={raw:?} is not on/1/true/yes or off/0/false/no: ignored, the setting decides");
    }
    parsed
}

/// `on`/`1`/`true`/`yes` -> on, `off`/`0`/`false`/`no` -> off, anything else -> no override
fn parse_override(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "1" | "true" | "yes" => Some(true),
        "off" | "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

/// How many tokens one draft may hold (`CLUAIZ_MTP_DRAFT`, else llama-server's 3)
pub fn draft_max() -> i32 {
    std::env::var("CLUAIZ_MTP_DRAFT").ok().and_then(|v| v.trim().parse().ok()).filter(|n| (1..=16).contains(n)).unwrap_or(DEFAULT_DRAFT_MAX)
}

/// The drafter's least probability for a guess (`CLUAIZ_MTP_PMIN`, 0..1); llama.cpp's default 0
/// drafts `draft_max` tokens whatever the drafter's confidence
pub fn p_min() -> f32 {
    std::env::var("CLUAIZ_MTP_PMIN").ok().and_then(|v| v.trim().parse().ok()).filter(|p: &f32| (0.0..1.0).contains(p)).unwrap_or(0.0)
}

/// llama.cpp's MTP drafter over a loaded model and its context. Freed before the context. Used only
/// by the thread that owns the context (every generation call holds `&mut NativeLlama`).
pub struct MtpDrafter {
    handle: *mut c_void,
    /// Tokens drafted and accepted since load, for the log
    drafted: AtomicU64,
    accepted: AtomicU64,
}

impl MtpDrafter {
    /// The drafter for `ctx` (made from `model`, loaded with `load_mtp`); the reason when it cannot be made
    pub fn new(model: *mut c_void, ctx: *mut c_void, n_max: i32) -> Result<Self, String> {
        let mut err = [0 as c_char; 256];
        // SAFETY: model and ctx are live llama.cpp handles; err has room for its length.
        let handle = unsafe { cluaiz_mtp_init(model, ctx, n_max, p_min(), err.as_mut_ptr(), err.len()) };
        if handle.is_null() {
            // SAFETY: the bridge writes a NUL-terminated string within the buffer.
            return Err(unsafe { CStr::from_ptr(err.as_ptr()) }.to_string_lossy().into_owned());
        }
        Ok(Self { handle, drafted: AtomicU64::new(0), accepted: AtomicU64::new(0) })
    }

    /// Tokens drafted and accepted since load
    pub fn totals(&self) -> (u64, u64) {
        (self.drafted.load(Ordering::Relaxed), self.accepted.load(Ordering::Relaxed))
    }

    pub fn n_max(&self) -> i32 {
        // SAFETY: live handle.
        unsafe { cluaiz_mtp_n_max(self.handle) }
    }

    /// Tells the drafter about a batch the target just decoded (sequence 0)
    pub fn process(&self, tokens: &[i32], pos: &[i32], output: &[i8]) -> bool {
        let n = tokens.len().min(pos.len()).min(output.len());
        // SAFETY: the three slices hold at least n entries.
        unsafe { cluaiz_mtp_process(self.handle, tokens.as_ptr(), pos.as_ptr(), output.as_ptr(), n as i32) }
    }

    /// The tokens of a batch the target decoded from position `start`, all in sequence 0
    pub fn process_run(&self, tokens: &[i32], start: i32, last_has_logits: bool) -> bool {
        let pos: Vec<i32> = (0..tokens.len() as i32).map(|i| start + i).collect();
        let mut output = vec![0i8; tokens.len()];
        if let (true, Some(last)) = (last_has_logits, output.last_mut()) {
            *last = 1;
        }
        self.process(tokens, &pos, &output)
    }

    /// A batch the target decoded from `start` with logits for every token (the guess check)
    pub fn process_run_all(&self, tokens: &[i32], start: i32) -> bool {
        let pos: Vec<i32> = (0..tokens.len() as i32).map(|i| start + i).collect();
        self.process(tokens, &pos, &vec![1i8; tokens.len()])
    }

    pub fn begin(&self, prompt: &[i32]) {
        // SAFETY: live handle; the slice is read during the call only.
        unsafe { cluaiz_mtp_begin(self.handle, prompt.as_ptr(), prompt.len() as i32) }
    }

    /// Up to `n_max` guesses after `id_last` (which goes in at `pos0`; `prompt` = the tokens before it)
    pub fn draft(&self, pos0: i32, id_last: i32, prompt: &[i32], n_max: i32) -> Vec<i32> {
        let cap = n_max.max(0) as usize;
        let mut out = vec![0i32; cap];
        // SAFETY: live handle; out has room for cap tokens.
        let n = unsafe {
            cluaiz_mtp_draft(self.handle, pos0, id_last, prompt.as_ptr(), prompt.len() as i32, n_max, out.as_mut_ptr(), cap as i32)
        };
        out.truncate(n.max(0) as usize);
        self.drafted.fetch_add(out.len() as u64, Ordering::Relaxed);
        out
    }

    pub fn accept(&self, n_accepted: usize) {
        self.accepted.fetch_add(n_accepted as u64, Ordering::Relaxed);
        // SAFETY: live handle.
        unsafe { cluaiz_mtp_accept(self.handle, n_accepted.min(u16::MAX as usize) as u16) }
    }

    /// Drops the draft context's tokens from `p0` on (the target's were dropped too)
    pub fn seq_rm(&self, p0: i32) {
        // SAFETY: live handle.
        unsafe { cluaiz_mtp_seq_rm(self.handle, p0.max(0)) }
    }
}

impl Drop for MtpDrafter {
    fn drop(&mut self) {
        // SAFETY: the handle came from cluaiz_mtp_init and is freed once.
        unsafe { cluaiz_mtp_free(self.handle) }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_override;

    #[test]
    fn mtp_override_values() {
        for on in ["on", "1", "true", "yes", " ON ", "Yes"] {
            assert_eq!(parse_override(on), Some(true), "{on:?}");
        }
        for off in ["off", "0", "false", "no", " Off ", "NO"] {
            assert_eq!(parse_override(off), Some(false), "{off:?}");
        }
        for other in ["", "auto", "2", "maybe"] {
            assert_eq!(parse_override(other), None, "{other:?}");
        }
    }
}
