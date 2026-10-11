//! 🎭 Native Dynamic Chat Templater: MiniJinja Native Engine with Safe Fallbacks

use std::ffi::{CStr, CString};
use crate::ffi::llama_cpp;

/// Template text llama.cpp renders with: the custom one when set, else the GGUF's own.
pub fn resolve_template(model_ptr: *const std::ffi::c_void, custom_template: Option<&str>) -> Option<String> {
    if let Some(custom) = custom_template.filter(|s| !s.trim().is_empty()) {
        return Some(custom.to_string());
    }
    if model_ptr.is_null() {
        return None;
    }
    let tmpl_ptr = unsafe { llama_cpp::llama_model_chat_template(model_ptr, std::ptr::null()) };
    if tmpl_ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(tmpl_ptr) }.to_str().ok().map(|s| s.to_string())
}

/// Parses the chat templates once for a loaded model (see `apply_chat_template`). Null on failure;
/// the caller frees it with `llama_chat_templates_handle_free` before freeing the model.
pub fn parse_templates(model_ptr: *const std::ffi::c_void, custom_template: Option<&str>) -> *mut std::ffi::c_void {
    let tmpl = resolve_template(model_ptr, custom_template).and_then(|t| CString::new(t).ok());
    let tmpl_ptr = tmpl.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
    unsafe { llama_cpp::llama_chat_templates_handle_init(model_ptr, tmpl_ptr) }
}

/// Applies chat template dynamically: Primary MiniJinja rendering directly on GGUF metadata,
/// ensuring zero alien ChatML token injection for non-ChatML architectures (like Gemma 4).
/// `templates`: handle from `parse_templates` for this same template, or null to parse it for this call.
pub fn apply_chat_template(
    model_ptr: *const std::ffi::c_void,
    templates: *const std::ffi::c_void,
    custom_template: Option<&str>,
    messages: &[(&str, &str)],
    add_generation_prompt: bool,
    enable_thinking: bool,
) -> anyhow::Result<String> {
    if messages.is_empty() {
        return Ok(String::new());
    }

    if model_ptr.is_null() {
        return Err(anyhow::anyhow!("Cannot format chat template: model pointer is null"));
    }

    // 1. Resolve template string from GGUF binary in RAM or custom override
    let tmpl_str = resolve_template(model_ptr, custom_template);

    
    // 2. Primary: Native llama.cpp common chat templates engine (supports ALL models natively)
    let c_custom_tmpl = tmpl_str.as_ref().and_then(|s| CString::new(s.as_str()).ok());
    let custom_ptr = c_custom_tmpl.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null());

    let mut c_roles = Vec::with_capacity(messages.len());
    let mut c_contents = Vec::with_capacity(messages.len());
    for (role, content) in messages {
        c_roles.push(CString::new(*role).unwrap_or_default());
        c_contents.push(CString::new(*content).unwrap_or_default());
    }

    let role_ptrs: Vec<*const std::os::raw::c_char> = c_roles.iter().map(|c| c.as_ptr()).collect();
    let content_ptrs: Vec<*const std::os::raw::c_char> = c_contents.iter().map(|c| c.as_ptr()).collect();

    // Without a parsed handle each call parses the whole template in llama.cpp, so render once into a
    // buffer sized from the inputs (the template's text plus the messages) and only retry when it was
    // too small. Both calls return the full prompt length either way.
    let input_len = tmpl_str.as_ref().map_or(0, |t| t.len())
        + messages.iter().map(|(r, c)| r.len() + c.len()).sum::<usize>();
    let render = |buf: &mut Vec<u8>| unsafe {
        let out = buf.as_mut_ptr() as *mut std::os::raw::c_char;
        if templates.is_null() {
            llama_cpp::llama_chat_apply_template_native(
                model_ptr, custom_ptr, role_ptrs.as_ptr(), content_ptrs.as_ptr(), messages.len(),
                add_generation_prompt, enable_thinking, out, buf.len(),
            )
        } else {
            llama_cpp::llama_chat_apply_template_handle(
                templates, role_ptrs.as_ptr(), content_ptrs.as_ptr(), messages.len(),
                add_generation_prompt, enable_thinking, out, buf.len(),
            )
        }
    };
    let started = std::time::Instant::now();
    let mut buf: Vec<u8> = vec![0u8; input_len * 2 + 1];
    let mut written = render(&mut buf);
    if written > 0 && written as usize >= buf.len() {
        buf = vec![0u8; written as usize + 1];
        written = render(&mut buf);
    }

    if written > 0 {
        let c_str = unsafe { CStr::from_ptr(buf.as_ptr() as *const std::os::raw::c_char) };
        let rendered = c_str.to_string_lossy().into_owned();
        if !rendered.trim().is_empty() {
            tracing::info!(
                "📝 [NativeTemplate] llama.cpp native common template rendered prompt ({} bytes) in {} ms",
                rendered.len(), started.elapsed().as_millis()
            );
            return Ok(rendered);
        }
    }

    // 3. Safe Clean Neutral Fallback: Never poison models with alien ChatML tokens
    let mut out = String::new();
    for (r, c) in messages {
        out.push_str(&format!("{}: {}\n", r, c));
    }
    if add_generation_prompt {
        out.push_str("assistant:\n");
    }
    tracing::info!("ℹ️ [NativeTemplate] Using clean neutral template fallback ({} bytes)", out.len());
    Ok(out)
}

/// Dynamically extracts thinking start and end tags from llama.cpp's native template engine.
/// Zero hardcoding: delegates completely to common_chat_params in llama.cpp.
pub fn extract_thinking_tags_native(
    model_ptr: *const std::ffi::c_void,
    custom_template: Option<&str>,
) -> (Option<String>, Option<String>) {
    let mut start_buf = vec![0u8; 128];
    let mut end_buf = vec![0u8; 128];

    let c_tmpl = custom_template.and_then(|s| CString::new(s).ok());
    let tmpl_ptr = c_tmpl.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null());

    let has_thinking = unsafe {
        llama_cpp::llama_chat_extract_thinking_tags(
            model_ptr,
            tmpl_ptr,
            start_buf.as_mut_ptr() as *mut std::os::raw::c_char,
            start_buf.len(),
            end_buf.as_mut_ptr() as *mut std::os::raw::c_char,
            end_buf.len(),
        )
    };

    if has_thinking {
        let start_str = unsafe { CStr::from_ptr(start_buf.as_ptr() as *const std::os::raw::c_char) }
            .to_string_lossy()
            .trim()
            .to_string();
        let end_str = unsafe { CStr::from_ptr(end_buf.as_ptr() as *const std::os::raw::c_char) }
            .to_string_lossy()
            .trim()
            .to_string();

        let s = if !start_str.is_empty() { Some(start_str) } else { None };
        let e = if !end_str.is_empty() { Some(end_str) } else { None };
        (s, e)
    } else {
        (None, None)
    }
}

