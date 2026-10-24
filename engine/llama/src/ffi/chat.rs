//! Narrow C ABI for native tool calls (`chat_bridge.cpp`): llama.cpp's common chat layer renders
//! tools in the model's own format, gives the lazy tool-call grammar and parses the output.

use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use std::ffi::{c_char, c_void, CStr, CString};

unsafe extern "C" {
    fn cluaiz_chat_templates_init(model: *const c_void, custom_tmpl: *const c_char) -> *mut c_void;
    fn cluaiz_chat_templates_free(tmpls: *mut c_void);
    fn cluaiz_chat_apply(tmpls: *const c_void, request: *const c_char) -> *mut c_char;
    fn cluaiz_chat_parser_init(apply: *const c_char) -> *mut c_void;
    fn cluaiz_chat_parser_free(parser: *mut c_void);
    fn cluaiz_chat_parse(parser: *const c_void, text: *const c_char, partial: bool) -> *mut c_char;
    fn cluaiz_chat_string_free(text: *mut c_char);
    fn cluaiz_tool_grammar_init(vocab: *const c_void, apply: *const c_char) -> *mut c_void;
    fn cluaiz_tool_grammar_set_active(smpl: *mut c_void, vocab: *const c_void, active: bool, replay: *const c_char);
}

/// Takes a string the bridge allocated and parses it as JSON
unsafe fn take_json(raw: *mut c_char) -> Result<Value> {
    if raw.is_null() {
        bail!("native chat bridge returned nothing");
    }
    // SAFETY: the bridge returns a NUL-terminated malloc'd string, freed right after the copy
    let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
    unsafe { cluaiz_chat_string_free(raw) };
    let value: Value = serde_json::from_str(&text)?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        bail!("{error}");
    }
    Ok(value)
}

/// The model's templates parsed for tool use (once per loaded model)
pub struct ChatTools(*mut c_void);

// SAFETY: used only by the generation thread that holds the model
unsafe impl Send for ChatTools {}

impl ChatTools {
    pub fn new(model: *const c_void, custom_template: Option<&str>) -> Option<Self> {
        let custom = custom_template.and_then(|t| CString::new(t).ok());
        let ptr = custom.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
        // SAFETY: the model outlives this handle (freed before the model)
        let handle = unsafe { cluaiz_chat_templates_init(model, ptr) };
        (!handle.is_null()).then_some(Self(handle))
    }

    /// Renders an OpenAI-style request (`messages`, `tools`, `enable_thinking`,
    /// `chat_template_kwargs`): the prompt and how to constrain and parse the answer
    pub fn apply(&self, request: &Value) -> Result<Value> {
        let request = CString::new(request.to_string())?;
        // SAFETY: live handle; the string lives for the call
        unsafe { take_json(cluaiz_chat_apply(self.0, request.as_ptr())) }
    }
}

impl Drop for ChatTools {
    fn drop(&mut self) {
        // SAFETY: created by cluaiz_chat_templates_init, freed once
        unsafe { cluaiz_chat_templates_free(self.0) }
    }
}

/// Reads the model's output back into content and tool calls (one generation)
pub struct ChatParser(*mut c_void);

impl ChatParser {
    /// `apply`: what `ChatTools::apply` returned
    pub fn new(apply: &Value) -> Result<Self> {
        let apply = CString::new(apply.to_string())?;
        // SAFETY: the string lives for the call
        let parser = unsafe { cluaiz_chat_parser_init(apply.as_ptr()) };
        if parser.is_null() {
            bail!("native chat parser could not be built");
        }
        Ok(Self(parser))
    }

    /// `text`: everything generated so far, thinking included. `partial` while generating.
    pub fn parse(&self, text: &str, partial: bool) -> Result<Value> {
        let text = CString::new(text).map_err(|_| anyhow!("output holds a NUL byte"))?;
        // SAFETY: live parser; the string lives for the call
        unsafe { take_json(cluaiz_chat_parse(self.0, text.as_ptr(), partial)) }
    }
}

impl Drop for ChatParser {
    fn drop(&mut self) {
        // SAFETY: created by cluaiz_chat_parser_init, freed once
        unsafe { cluaiz_chat_parser_free(self.0) }
    }
}

/// The tool-call grammar sampler (owned by the chain it is added to). Null when the request has
/// no grammar or llama.cpp cannot build it.
///
/// # Safety
/// `vocab` is the loaded model's vocabulary.
pub unsafe fn tool_grammar(vocab: *const c_void, apply: &Value) -> *mut c_void {
    let Ok(apply) = CString::new(apply.to_string()) else {
        return std::ptr::null_mut();
    };
    // SAFETY: caller guarantees the vocab; the string lives for the call
    unsafe { cluaiz_tool_grammar_init(vocab, apply.as_ptr()) }
}

/// Switches the grammar on outside thinking (replaying `replay`, the thinking end tag) or off
///
/// # Safety
/// `grammar` is live (its chain is) and used by this generation thread only.
pub unsafe fn set_tool_grammar_active(grammar: *mut c_void, vocab: *const c_void, active: bool, replay: &str) {
    let replay = CString::new(replay).unwrap_or_default();
    // SAFETY: caller guarantees the sampler and vocab
    unsafe { cluaiz_tool_grammar_set_active(grammar, vocab, active, replay.as_ptr()) }
}
