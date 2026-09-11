//! 🎭 Native Dynamic Chat Templater: Direct C-Bridge to `llama_chat_apply_template`
 
use std::ffi::{CStr, CString};
use crate::ffi::llama_cpp::{self, LlamaChatMessage};

/// Applies chat template using native llama.cpp engine directly on the in-memory GGUF model metadata.
pub fn apply_chat_template(
    model_ptr: *const std::ffi::c_void,
    custom_template: Option<&str>,
    messages: &[(&str, &str)],
    add_generation_prompt: bool,
) -> anyhow::Result<String> {
    if messages.is_empty() {
        return Ok(String::new());
    }

    if model_ptr.is_null() {
        return Err(anyhow::anyhow!("Cannot format chat template: model pointer is null"));
    }

    // 1. Primary: Retrieve model-native template directly from GGUF binary metadata in RAM
    let tmpl_ptr = unsafe { llama_cpp::llama_model_chat_template(model_ptr, std::ptr::null()) };

    let mut _custom_holder: Option<CString> = None;
    let mut active_tmpl = if !tmpl_ptr.is_null() {
        tmpl_ptr
    } else if let Some(custom) = custom_template.filter(|s| !s.trim().is_empty()) {
        let c = CString::new(custom).unwrap_or_default();
        let ptr = c.as_ptr();
        _custom_holder = Some(c);
        ptr
    } else {
        std::ptr::null()
    };

    // 2. Prepare CStrings for roles and contents
    let mut c_roles = Vec::with_capacity(messages.len());
    let mut c_contents = Vec::with_capacity(messages.len());
    for (role, content) in messages {
        c_roles.push(CString::new(*role).unwrap_or_default());
        c_contents.push(CString::new(*content).unwrap_or_default());
    }

    let mut chat_messages: Vec<LlamaChatMessage> = Vec::with_capacity(messages.len());
    for i in 0..messages.len() {
        chat_messages.push(LlamaChatMessage {
            role: c_roles[i].as_ptr(),
            content: c_contents[i].as_ptr(),
        });
    }

    // 3. Query required buffer length from llama.cpp's native Minja engine
    let mut required_len = unsafe {
        llama_cpp::llama_chat_apply_template(
            active_tmpl,
            chat_messages.as_ptr(),
            chat_messages.len(),
            add_generation_prompt,
            std::ptr::null_mut(),
            0,
        )
    };

    // If custom/embedded template returned negative and active_tmpl was not null, try llama.cpp internal architecture template
    if required_len < 0 && !active_tmpl.is_null() {
        active_tmpl = std::ptr::null();
        required_len = unsafe {
            llama_cpp::llama_chat_apply_template(
                active_tmpl,
                chat_messages.as_ptr(),
                chat_messages.len(),
                add_generation_prompt,
                std::ptr::null_mut(),
                0,
            )
        };
    }

    if required_len < 0 {
        return Err(anyhow::anyhow!(
            "llama_chat_apply_template failed with error code: {} (tmpl_ptr is_null: {})",
            required_len,
            tmpl_ptr.is_null()
        ));
    }

    // 4. Allocate buffer and render exact template
    let mut buf: Vec<u8> = vec![0u8; (required_len + 16) as usize];
    let written = unsafe {
        llama_cpp::llama_chat_apply_template(
            active_tmpl,
            chat_messages.as_ptr(),
            chat_messages.len(),
            add_generation_prompt,
            buf.as_mut_ptr() as *mut std::os::raw::c_char,
            buf.len() as i32,
        )
    };

    if written < 0 {
        return Err(anyhow::anyhow!("llama_chat_apply_template buffer write failed with code: {}", written));
    }

    let c_str = unsafe { CStr::from_ptr(buf.as_ptr() as *const std::os::raw::c_char) };
    let rendered = c_str.to_string_lossy().into_owned();
    eprintln!("📝 [NativeTemplate] llama.cpp rendered prompt ({} bytes):\n{}", rendered.len(), rendered);
    tracing::info!("📝 [NativeTemplate] llama.cpp rendered prompt ({} bytes):\n{}", rendered.len(), rendered);
    Ok(rendered)
}
