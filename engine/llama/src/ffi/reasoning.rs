//! Narrow C ABI for the pinned upstream reasoning sampler; no copied sampler logic.

use anyhow::{bail, Result};
use std::ffi::{c_char, c_void, CString};

unsafe extern "C" {
    fn cluaiz_reasoning_wrap(
        chain: *mut c_void,
        vocab: *const c_void,
        start: *const c_char,
        end: *const c_char,
        forced: *const c_char,
        budget: i32,
        inside: bool,
        reasoning: *mut *mut c_void,
    ) -> *mut c_void;
    fn cluaiz_reasoning_force(sampler: *mut c_void) -> bool;
    fn cluaiz_reasoning_is_forcing(sampler: *const c_void) -> bool;
}

/// Borrowed from the composed chain; the generation owner must free that chain last.
pub(crate) struct ReasoningHandle(*mut c_void);

impl ReasoningHandle {
    /// # Safety
    /// The owning chain is live and exclusively used by this generation thread.
    pub(crate) unsafe fn poll(&self, requested: bool, ended_by: &mut &'static str) {
        // SAFETY: this method has the same chain lifetime as force/is_forcing.
        if requested && unsafe { self.force() } {
            *ended_by = "skip";
            tracing::info!("Native reasoning sampler accepted this request's skip signal");
        } else if *ended_by != "skip" && unsafe { self.is_forcing() } {
            *ended_by = "budget";
        }
    }

    /// # Safety
    /// All handles are live, exclusively used by this generation. On success the
    /// returned chain owns the old chain; on failure the caller still owns it.
    pub(crate) unsafe fn wrap(
        chain: *mut c_void,
        vocab: *const c_void,
        tags: (&str, &str, &str),
        budget: usize,
        inside: bool,
    ) -> Result<(*mut c_void, Self)> {
        let (start, end, forced) = (
            CString::new(tags.0)?,
            CString::new(tags.1)?,
            CString::new(tags.2)?,
        );
        let mut borrowed = std::ptr::null_mut();
        // SAFETY: caller retains native chain/vocab; C strings live for this call.
        let root = unsafe {
            cluaiz_reasoning_wrap(
                chain,
                vocab,
                start.as_ptr(),
                end.as_ptr(),
                forced.as_ptr(),
                i32::try_from(budget).unwrap_or(i32::MAX),
                inside,
                &mut borrowed,
            )
        };
        if root.is_null() {
            bail!("Native reasoning sampler initialization failed");
        }
        Ok((root, Self(borrowed)))
    }

    /// # Safety
    /// The owning composed chain must still be alive on this generation thread.
    pub(crate) unsafe fn force(&self) -> bool {
        // SAFETY: lifetime/thread exclusivity is required by the caller.
        !self.0.is_null() && unsafe { cluaiz_reasoning_force(self.0) }
    }

    /// # Safety
    /// The owning composed chain must still be alive on this generation thread.
    pub(crate) unsafe fn is_forcing(&self) -> bool {
        // SAFETY: lifetime/thread exclusivity is required by the caller.
        unsafe { cluaiz_reasoning_is_forcing(self.0) }
    }
}
