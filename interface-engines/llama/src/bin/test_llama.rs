use cluaiz_llama::ffi::llama_cpp::{llama_model_default_params, llama_model_load_from_file, llama_log_set};
use std::ffi::CString;

extern "C" fn log_cb(_level: std::ffi::c_int, text: *const std::ffi::c_char, _ud: *mut std::ffi::c_void) {
    unsafe {
        if text.is_null() { return; }
        println!("LLAMA_LOG: {}", std::ffi::CStr::from_ptr(text).to_string_lossy());
    }
}

fn main() {
    unsafe { llama_log_set(Some(log_cb), std::ptr::null_mut()); }
    let path = CString::new("C:\\Users\\Aryan\\.cluaiz\\models\\chat\\deepseek_coder_v2_lite_instruct-unknown-gguf-q4_k_m\\DeepSeek-Coder-V2-Lite-Instruct-Q4_K_M.gguf").unwrap();
    let mut params = unsafe { llama_model_default_params() };
    params.n_gpu_layers = 0;
    println!("Loading model...");
    let ptr = unsafe { llama_model_load_from_file(path.as_ptr(), params) };
    if ptr.is_null() { println!("FAILED TO LOAD"); return; }
    let mut ctx_params = unsafe { cluaiz_llama::ffi::llama_cpp::llama_context_default_params() };
    ctx_params.type_k = 1;
    ctx_params.type_v = 1;
    ctx_params.flash_attn_type = 1;
    ctx_params.n_ctx = 8192;
    let ctx = unsafe { cluaiz_llama::ffi::llama_cpp::llama_init_from_model(ptr, ctx_params) };
    if ctx.is_null() { println!("FAILED TO INIT CONTEXT"); }
    if ptr.is_null() {
        println!("FAILED TO LOAD");
    } else {
        println!("SUCCESS");
    }
}
