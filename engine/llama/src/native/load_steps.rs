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
}
