//! What a chat-model load is doing right now, for whoever started it on this thread: the share of
//! the weights read (llama.cpp's load progress callback), a fallback llama.cpp's own load took, the
//! context (KV cache and compute buffers) being made, the warmup. The load runs synchronously on
//! one thread, so the observer is thread-local and needs no user data pointer in llama.cpp.

use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq)]
pub enum LoadStep {
    /// Weights read so far, in percent (each whole percent once)
    Weights(u8),
    /// The load was tried again in another way (fewer GPU layers, CPU only, without mmap, without
    /// flash attention); the text says which
    Fallback(&'static str),
    /// Making the context: KV cache and compute buffers
    Context,
    /// One decode so the GPU memory pool and kernels are ready before the first message
    Warmup,
}

type Observer = Box<dyn FnMut(LoadStep)>;

thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
    static LAST_PERCENT: RefCell<Option<u8>> = const { RefCell::new(None) };
    /// The last error line llama.cpp logged on this thread: why a load failed
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// A chat-model load llama.cpp refused, with its own reason
#[derive(Debug, Clone, PartialEq)]
pub struct LoadFailure {
    /// `model` (the weights) or `context` (KV cache and compute buffers)
    pub stage: &'static str,
    /// llama.cpp's last error line, when it logged one
    pub reason: Option<String>,
}

impl LoadFailure {
    /// Whether memory ran out (or llama.cpp gave no reason, which may be memory): only then can a
    /// smaller placement help
    pub fn is_memory(&self) -> bool {
        self.reason.as_deref().is_none_or(|reason| {
            let reason = reason.to_ascii_lowercase();
            reason.contains("alloc") || reason.contains("out of memory")
        })
    }

    /// The failure with the reason llama.cpp logged on this thread since the load began
    pub(crate) fn now(stage: &'static str) -> Self {
        Self { stage, reason: LAST_ERROR.with(|last| last.borrow_mut().take()) }
    }
}

impl std::fmt::Display for LoadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = if self.stage == "model" { "Model Load Failure" } else { "Context Init Failure" };
        match &self.reason {
            Some(reason) => write!(f, "{what}: {reason}"),
            None => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for LoadFailure {}

/// Keeps a llama.cpp error line as this thread's last; llama.cpp's note that the line is normal
/// while it fits memory is left out, since it is not true for a real load
pub(crate) fn note_error(line: &str) {
    let line = line.trim_end_matches("(this warning is normal during memory fitting)").trim_end();
    let line = line.strip_prefix("llama_init_from_model: ").unwrap_or(line);
    LAST_ERROR.with(|last| *last.borrow_mut() = Some(line.to_string()));
}

/// Forgets earlier error lines of this thread (a load begins)
pub(crate) fn clear_error() {
    LAST_ERROR.with(|last| *last.borrow_mut() = None);
}

/// Runs `work` with `observer` told each step of a load made on this thread inside it
pub fn observe<T>(observer: impl FnMut(LoadStep) + 'static, work: impl FnOnce() -> T) -> T {
    let previous = OBSERVER.with(|slot| slot.borrow_mut().replace(Box::new(observer)));
    LAST_PERCENT.with(|last| *last.borrow_mut() = None);
    let result = work();
    OBSERVER.with(|slot| *slot.borrow_mut() = previous);
    result
}

/// Whether a load on this thread has an observer (the progress callback is set only then)
pub(crate) fn observed() -> bool {
    OBSERVER.with(|slot| slot.borrow().is_some())
}

pub(crate) fn emit(step: LoadStep) {
    if let LoadStep::Weights(percent) = step {
        let repeated = LAST_PERCENT.with(|last| last.borrow_mut().replace(percent) == Some(percent));
        if repeated {
            return;
        }
    }
    OBSERVER.with(|slot| {
        if let Some(observer) = slot.borrow_mut().as_mut() {
            observer(step);
        }
    });
}

/// llama.cpp's `progress_callback`: called on the loading thread while it reads the weights.
/// Returning false would cancel the load, so it always returns true.
pub(crate) extern "C" fn weights_progress(progress: f32, _user_data: *mut std::ffi::c_void) -> bool {
    emit(LoadStep::Weights((progress.clamp(0.0, 1.0) * 100.0) as u8));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn steps_reach_the_observer_once_per_percent_and_only_inside_observe() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        emit(LoadStep::Context);
        observe(move |step| sink.lock().unwrap().push(step), || {
            for progress in [0.0, 0.001, 0.5, 0.501, 1.0] {
                weights_progress(progress, std::ptr::null_mut());
            }
            emit(LoadStep::Warmup);
        });
        emit(LoadStep::Context);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![LoadStep::Weights(0), LoadStep::Weights(50), LoadStep::Weights(100), LoadStep::Warmup]
        );
        assert!(!observed());
    }

    #[test]
    fn a_failure_carries_the_last_error_line_and_says_whether_memory_ran_out() {
        clear_error();
        note_error("ggml_backend_buft_alloc_buffer_n_default: failed to allocate CUDA0 buffer of size 1610612736");
        note_error("llama_init_from_model: failed to initialize the context: failed to allocate buffer for kv cache");
        let memory = LoadFailure::now("context");
        assert_eq!(memory.to_string(), "Context Init Failure: failed to initialize the context: failed to allocate buffer for kv cache");
        assert!(memory.is_memory());
        note_error("llama_init_from_model: failed to initialize the context: dflash requires ctx_other to be set (this warning is normal during memory fitting)");
        let other = LoadFailure::now("context");
        assert_eq!(other.reason.as_deref(), Some("failed to initialize the context: dflash requires ctx_other to be set"));
        assert!(!other.is_memory());
        assert!(LoadFailure::now("model").is_memory(), "no reason logged: may be memory");
    }
}
