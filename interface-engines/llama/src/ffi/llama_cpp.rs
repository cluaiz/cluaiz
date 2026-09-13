//! 🏛️ Sovereign FFI: Industrial Llama.cpp Bindings
//! This module provides direct access to the core llama.cpp parameters and lifecycle functions.

use std::os::raw::{c_char, c_int, c_float};

pub const GGML_TYPE_F16: i32 = 1;
pub const GGML_TYPE_Q8_0: i32 = 8;
pub const GGML_TYPE_Q4_0: i32 = 2;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LlamaModelParams {
    pub devices: *const usize, // ggml_backend_dev_t*
    pub tensor_buft_overrides: *const std::ffi::c_void,
    pub n_gpu_layers: i32,
    pub split_mode: i32,
    pub load_mode: i32, // enum llama_load_mode (0=none, 1=mmap, 2=mlock, 3=mmap+mlock, 4=direct_io)
    pub lazy_mode: i32, // enum llama_lazy_mode (0=off, 1=auto, 2=on)
    pub main_gpu: i32,
    pub tensor_split: *const f32,
    pub progress_callback: Option<extern "C" fn(f32, *mut std::ffi::c_void) -> bool>,
    pub progress_callback_user_data: *mut std::ffi::c_void,
    pub kv_overrides: *const std::ffi::c_void,
    pub vocab_only: bool,
    pub check_tensors: bool,
    pub use_extra_bufts: bool,
    pub no_host: bool,
    pub no_alloc: bool,
    pub load_mtp: bool,
}

impl LlamaModelParams {
    #[inline]
    pub fn is_mmap(&self) -> bool {
        self.load_mode == 1 || self.load_mode == 3
    }

    #[inline]
    pub fn is_mlock(&self) -> bool {
        self.load_mode == 2 || self.load_mode == 3
    }

    #[inline]
    pub fn set_mmap(&mut self, mmap: bool) {
        let mlock = self.is_mlock();
        self.load_mode = match (mmap, mlock) {
            (true, true) => 3,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 0,
        };
    }

    #[inline]
    pub fn set_mlock(&mut self, mlock: bool) {
        let mmap = self.is_mmap();
        self.load_mode = match (mmap, mlock) {
            (true, true) => 3,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 0,
        };
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LlamaContextParams {
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub n_seq_max: u32,
    pub n_rs_seq: u32,
    pub n_outputs_max: u32,
    pub n_outputs_max_per_seq: u32,
    pub n_threads: i32,
    pub n_threads_batch: i32,
    pub ctx_type: i32, // enum llama_context_type
    pub rope_scaling_type: i32,
    pub pooling_type: i32,
    pub attention_type: i32,
    pub flash_attn_type: i32,
    pub rope_freq_base: f32,
    pub rope_freq_scale: f32,
    pub yarn_ext_factor: f32,
    pub yarn_attn_factor: f32,
    pub yarn_beta_fast: f32,
    pub yarn_beta_slow: f32,
    pub yarn_orig_ctx: u32,
    pub defrag_thold: f32,
    pub cb_eval: *mut std::ffi::c_void, // ggml_backend_sched_eval_callback
    pub cb_eval_user_data: *mut std::ffi::c_void,
    pub type_k: i32,
    pub type_v: i32,
    pub abort_callback: *mut std::ffi::c_void,
    pub abort_callback_data: *mut std::ffi::c_void,

    // Keep the booleans together and at the end of the struct to avoid misalignment during copy-by-value.
    pub embeddings: u8,
    pub offload_kqv: u8,
    pub no_perf: u8,
    pub op_offload: u8,
    pub swa_full: u8,
    pub kv_unified: u8,
    pub _pad: [u8; 2], // Pad to 8-byte boundary for the next pointer

    // [EXPERIMENTAL]
    pub samplers: *mut std::ffi::c_void,
    pub n_samplers: usize,
    pub ctx_other: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LlamaChatMessage {
    pub role: *const c_char,
    pub content: *const c_char,
}

extern "C" {
    /// 🚀 Initialize the global llama + ggml backend.
    pub fn llama_backend_init();
    
    /// 🛑 Free the global llama + ggml backend.
    pub fn llama_backend_free();

    /// 🔥 Sovereign Injection: Manually register a backend (bypass CMake CMake auto-detection).
    pub fn ggml_backend_register(reg: *mut std::ffi::c_void);

    /// 🔥 Sovereign Injection: Get CUDA backend registry
    pub fn ggml_backend_cuda_reg() -> *mut std::ffi::c_void;

    /// 🛠️ Get default model parameters.
    pub fn llama_model_default_params() -> LlamaModelParams;

    /// 🛠️ Get default context parameters.
    pub fn llama_context_default_params() -> LlamaContextParams;

    /// 🧬 Load model from file.
    pub fn llama_model_load_from_file(
        path_model: *const c_char,
        params: LlamaModelParams,
    ) -> *mut std::ffi::c_void;

    /// 🧬 Initialize context from model.
    pub fn llama_init_from_model(
        model: *mut std::ffi::c_void,
        params: LlamaContextParams,
    ) -> *mut std::ffi::c_void;

    /// 🗑️ Free model memory.
    pub fn llama_model_free(model: *mut std::ffi::c_void);

    /// 🗑️ Free context memory.
    pub fn llama_free(ctx: *mut std::ffi::c_void);

    /// 🔤 Tokenize text.
    pub fn llama_tokenize(
        vocab: *const std::ffi::c_void,
        text: *const c_char,
        text_len: i32,
        tokens: *mut i32,
        n_max_tokens: i32,
        add_special: bool,
        parse_special: bool,
    ) -> i32;

    /// 🧬 Get model vocabulary.
    pub fn llama_model_get_vocab(model: *const std::ffi::c_void) -> *const std::ffi::c_void;
    pub fn llama_vocab_n_tokens(vocab: *const std::ffi::c_void) -> i32;

    /// 🏛️ Initialize a batch for inference.
    pub fn llama_batch_init(n_tokens: i32, embd: i32, n_seq_max: i32) -> LlamaBatch;

    /// 🏛️ Free batch memory.
    pub fn llama_batch_free(batch: LlamaBatch);

    /// ⚡ Execute a decode pass.
    pub fn llama_decode(ctx: *mut std::ffi::c_void, batch: LlamaBatch) -> i32;

    /// 📐 Extract Embeddings
    pub fn llama_get_embeddings(ctx: *mut std::ffi::c_void) -> *mut c_float;
    pub fn llama_model_n_embd(model: *const std::ffi::c_void) -> i32;
    pub fn llama_model_n_head(model: *const std::ffi::c_void) -> i32;
    pub fn llama_model_is_recurrent(model: *const std::ffi::c_void) -> bool;
    pub fn llama_model_is_hybrid(model: *const std::ffi::c_void) -> bool;
    pub fn llama_model_n_expert(model: *const std::ffi::c_void) -> i32;
    pub fn llama_model_desc(model: *const std::ffi::c_void, buf: *mut c_char, buf_size: usize) -> i32;
    pub fn llama_model_meta_val_str(
        model: *const std::ffi::c_void,
        key: *const c_char,
        buf: *mut c_char,
        buf_size: usize,
    ) -> i32;

    /// 📊 Get Logits
    pub fn llama_get_logits_ith(ctx: *mut std::ffi::c_void, i: i32) -> *mut c_float;

    /// 🎲 Industrial Sampler Suite
    pub fn llama_sampler_chain_init(params: LlamaSamplerChainParams) -> *mut std::ffi::c_void;
    pub fn llama_sampler_chain_add(chain: *mut std::ffi::c_void, sampler: *mut std::ffi::c_void);
    pub fn llama_sampler_init_greedy() -> *mut std::ffi::c_void;
    pub fn llama_sampler_init_top_k(k: i32) -> *mut std::ffi::c_void;
    pub fn llama_sampler_init_top_p(p: f32, min_keep: usize) -> *mut std::ffi::c_void;
    pub fn llama_sampler_init_min_p(p: f32, min_keep: usize) -> *mut std::ffi::c_void;
    pub fn llama_sampler_init_temp(t: f32) -> *mut std::ffi::c_void;
    pub fn llama_sampler_init_dist(seed: u32) -> *mut std::ffi::c_void;
    pub fn llama_sampler_init_penalties(
        n_vocab: i32,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> *mut std::ffi::c_void;
    pub fn llama_sampler_sample(sampler: *mut std::ffi::c_void, ctx: *mut std::ffi::c_void, idx: i32) -> i32;
    pub fn llama_sampler_accept(sampler: *mut std::ffi::c_void, token: i32);
    pub fn llama_sampler_free(sampler: *mut std::ffi::c_void);

    /// 🧵 Memory Sequence Management (Signal Stitching)
    pub fn llama_get_memory(ctx: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    pub fn llama_memory_seq_add(mem: *mut std::ffi::c_void, seq_id: i32, p0: i32, p1: i32, delta: i32);
    pub fn llama_memory_seq_cp(mem: *mut std::ffi::c_void, seq_id_src: i32, seq_id_dst: i32, p0: i32, p1: i32);
    pub fn llama_memory_seq_rm(mem: *mut std::ffi::c_void, seq_id: i32, p0: i32, p1: i32) -> bool;
    pub fn llama_memory_seq_pos_max(mem: *mut std::ffi::c_void, seq_id: i32) -> i32;

    /// 🔤 Convert token to piece (string).
    pub fn llama_token_to_piece(
        vocab: *const std::ffi::c_void,
        token: i32,
        buf: *mut c_char,
        length: i32,
        lstrip: i32,
        special: bool,
    ) -> i32;

    /// 🏁 EOS/EOG Detection
    pub fn llama_vocab_is_eog(vocab: *const std::ffi::c_void, token: i32) -> bool;
    pub fn llama_vocab_is_control(vocab: *const std::ffi::c_void, token: i32) -> bool;
    pub fn llama_vocab_get_add_bos(vocab: *const std::ffi::c_void) -> bool;
    pub fn llama_vocab_eos(vocab: *const std::ffi::c_void) -> i32;
    pub fn llama_vocab_eot(vocab: *const std::ffi::c_void) -> i32;
    pub fn llama_vocab_nl(vocab: *const std::ffi::c_void) -> i32;

    /// 🧬 Metadata Extraction
    pub fn llama_model_meta_count(model: *const std::ffi::c_void) -> i32;
    pub fn llama_model_meta_key_by_index(model: *const std::ffi::c_void, i: i32, buf: *mut c_char, buf_size: usize) -> i32;
    pub fn llama_model_meta_val_str_by_index(model: *const std::ffi::c_void, i: i32, buf: *mut c_char, buf_size: usize) -> i32;
    pub fn llama_model_chat_template(model: *const std::ffi::c_void, name: *const c_char) -> *const c_char;
    pub fn llama_chat_apply_template(
        tmpl: *const c_char,
        chat: *const LlamaChatMessage,
        n_msg: usize,
        add_ass: bool,
        buf: *mut c_char,
        length: i32,
    ) -> i32;

    /// 🛑 Logging: Redirect native library logs to avoid TUI noise.
    pub fn llama_log_set(log_callback: Option<LlamaLogCallback>, user_data: *mut std::ffi::c_void);

    /// 💾 State Save
    pub fn llama_state_save_file(
        ctx: *mut std::ffi::c_void,
        path_session: *const c_char,
        tokens: *const i32,
        n_token_count: usize,
    ) -> bool;

    pub fn llama_state_seq_save_file(
        ctx: *mut std::ffi::c_void,
        path_session: *const c_char,
        seq_id: i32,
        tokens: *const i32,
        n_token_count: usize,
    ) -> usize;

    /// 💾 State Load
    pub fn llama_state_load_file(
        ctx: *mut std::ffi::c_void,
        path_session: *const c_char,
        tokens_out: *mut i32,
        n_token_capacity: usize,
        n_token_count_out: *mut usize,
    ) -> bool;

    pub fn llama_state_seq_load_file(
        ctx: *mut std::ffi::c_void,
        path_session: *const c_char,
        seq_id: i32,
        tokens_out: *mut i32,
        n_token_capacity: usize,
        n_token_count_out: *mut usize,
    ) -> usize;

    pub fn ggml_get_name(tensor: *const std::ffi::c_void) -> *const c_char;

    pub fn llama_chat_extract_thinking_tags(
        model: *const std::ffi::c_void,
        custom_tmpl: *const c_char,
        out_start: *mut c_char,
        max_start_len: usize,
        out_end: *mut c_char,
        max_end_len: usize,
    ) -> bool;

    pub fn llama_chat_apply_template_native(
        model: *const std::ffi::c_void,
        custom_tmpl: *const c_char,
        roles: *const *const c_char,
        contents: *const *const c_char,
        n_messages: usize,
        add_ass: bool,
        enable_thinking: bool,
        buf: *mut c_char,
        buf_size: usize,
    ) -> i32;
}

pub type LlamaLogCallback = extern "C" fn(level: i32, text: *const c_char, user_data: *mut std::ffi::c_void);

/// ✅ Matches `llama_sampler_chain_params` in llama.h exactly.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LlamaSamplerChainParams {
    pub no_perf: bool, // whether to measure performance timings
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LlamaBatch {
    pub n_tokens: i32,
    pub token: *mut i32,
    pub embd: *mut f32,
    pub pos: *mut i32,
    pub n_seq_id: *mut i32,
    pub seq_id: *mut *mut i32,
    pub logits: *mut i8,
}
