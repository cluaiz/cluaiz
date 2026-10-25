//! The Cluaiz Engine as a C library (01-architecture §10.3): two functions, one JSON-RPC 2.0
//! message in and out, exactly as `POST /v1/cep`.
//!
//! ```c
//! char *cep_call(const char *request_json);  // reply JSON, or NULL for notifications only
//! void  cep_free(char *reply);               // frees what cep_call returned
//! ```
//!
//! The first call starts the Engine inside the calling process (home from `CLUAIZ_HOME`, or the
//! default). Calls come from the process itself, so the caller is the owner. Long work returns a
//! task handle (`tasks.*`), so no callback API is needed.

use cluaiz_api::cep::CallCtx;
use cluaiz_api::AppState;
use cluaiz_cep::{envelope, CepError, Principal};
use std::ffi::{c_char, CStr, CString};
use std::sync::{Arc, OnceLock};

struct Engine {
    runtime: tokio::runtime::Runtime,
    state: Arc<AppState>,
    owner: Arc<Principal>,
}

static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();

fn engine() -> Result<&'static Engine, String> {
    ENGINE
        .get_or_init(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| format!("Engine runtime did not start: {e}"))?;
            // Inside the runtime: starting the state may spawn tasks
            let state = runtime.block_on(async { cluaiz_api::boot::boot_in_process() });
            Ok(Engine { runtime, state: Arc::new(state), owner: Arc::new(Principal::owner()) })
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn error_reply(error: &CepError) -> Vec<u8> {
    serde_json::to_vec(&envelope::err(serde_json::Value::Null, error)).unwrap_or_default()
}

/// One request or batch; `None` when it held only notifications
fn call(request: &[u8]) -> Option<Vec<u8>> {
    if tokio::runtime::Handle::try_current().is_ok() {
        let error = CepError::new(
            cluaiz_cep::error::INTERNAL_ERROR,
            "cep_call blocks until the answer; call it from a thread that is not running an async runtime",
            None,
        );
        return Some(error_reply(&error));
    }
    let engine = match engine() {
        Ok(engine) => engine,
        Err(message) => return Some(error_reply(&CepError::unavailable("engine_start_failed", message))),
    };
    let ctx = CallCtx { state: engine.state.clone() };
    engine.runtime.block_on(engine.state.cep.call(ctx, engine.owner.clone(), request))
}

/// A panic never crosses into the caller: it becomes an internal-error reply
fn guarded(run: impl FnOnce() -> Option<Vec<u8>> + std::panic::UnwindSafe) -> Option<Vec<u8>> {
    std::panic::catch_unwind(run).unwrap_or_else(|_| Some(error_reply(&CepError::internal("ffi_panic"))))
}

/// # Safety
/// `request_json` is NULL or a NUL-terminated string that stays valid during the call. The
/// returned string must be released with `cep_free`, not with the C runtime's `free`.
#[no_mangle]
pub unsafe extern "C" fn cep_call(request_json: *const c_char) -> *mut c_char {
    let reply = guarded(|| {
        if request_json.is_null() {
            return Some(error_reply(&CepError::parse_error()));
        }
        // SAFETY: the caller passes a valid NUL-terminated string (see above)
        call(unsafe { CStr::from_ptr(request_json) }.to_bytes())
    });
    match reply {
        // JSON text never holds a raw NUL (serde writes it as \u0000)
        Some(bytes) => CString::new(bytes).map(CString::into_raw).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `reply` is NULL or a pointer `cep_call` returned that was not freed yet.
#[no_mangle]
pub unsafe extern "C" fn cep_free(reply: *mut c_char) {
    if !reply.is_null() {
        // SAFETY: it came from CString::into_raw in cep_call
        drop(unsafe { CString::from_raw(reply) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_panic_becomes_an_internal_error_reply() {
        let reply = guarded(|| panic!("boom")).unwrap();
        let reply: serde_json::Value = serde_json::from_slice(&reply).unwrap();
        assert_eq!(reply["error"]["code"], cluaiz_cep::error::INTERNAL_ERROR);
    }

    #[test]
    fn test_a_call_from_an_async_runtime_thread_is_an_error_not_a_deadlock() {
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let reply = runtime.block_on(async { call(br#"{"jsonrpc":"2.0","id":1,"method":"tasks.list"}"#) }).unwrap();
        let reply: serde_json::Value = serde_json::from_slice(&reply).unwrap();
        assert_eq!(reply["error"]["code"], cluaiz_cep::error::INTERNAL_ERROR);
        assert!(reply["error"]["message"].as_str().unwrap().contains("async runtime"));
    }

    #[test]
    fn test_null_input_and_free_null() {
        unsafe {
            let reply = cep_call(std::ptr::null());
            let text = CStr::from_ptr(reply).to_str().unwrap().to_string();
            cep_free(reply);
            cep_free(std::ptr::null_mut());
            assert!(text.contains("-32700"), "{text}");
        }
    }
}
