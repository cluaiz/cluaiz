//! Speech from llama.cpp: a text-to-speech backbone GGUF and its audio projector (mmproj) make a
//! WAV with libmtmd's audio generation, the loop of llama.cpp's `tools/tts`. Upstream marks this
//! API experimental, so every call into it is in this module (and its declarations in `ffi::mtmd`).
//!
//! The backbone runs in its own model and context, separate from the chat model, so speaking never
//! touches a chat's state.

use crate::ffi::llama_cpp;
use crate::ffi::mtmd;
use anyhow::{bail, Context, Result};
use std::ffi::{c_void, CString};
use std::path::Path;
use std::ptr::NonNull;

/// Sampling of the backbone's speech codes; the values of llama.cpp's defaults, which `tools/tts`
/// uses.
const TOP_K: i32 = 40;
const TOP_P: f32 = 0.95;
const MIN_P: f32 = 0.05;
const TEMPERATURE: f32 = 0.8;
/// Prompt tokens evaluated per step
const BATCH: i32 = 512;
/// Context for the prompt and the generated frames
const CONTEXT: u32 = 4096;

/// A loaded speech backbone with its audio projector.
pub struct SpeechModel {
    model: NonNull<c_void>,
    context: NonNull<c_void>,
    projector: NonNull<mtmd::mtmd_context>,
    sample_rate: u32,
}

// SAFETY: the native handles are owned by this value and used by one thread at a time (callers
// hold it behind a mutex).
unsafe impl Send for SpeechModel {}

/// Options of one utterance.
#[derive(Debug, Clone, Default)]
pub struct SpeechOptions {
    /// Language code for models that take one (Qwen3-TTS: en, zh, ja, ko, de, fr, ru, pt, es, it)
    pub language: Option<String>,
    /// Upper bound on audio frames; None: enough for the text
    pub max_frames: Option<usize>,
    /// None: a new random voice each time (the model's own behaviour without a speaker sample)
    pub seed: Option<u32>,
}

impl SpeechModel {
    /// Loads the backbone (all layers on the GPU when `gpu`) and the projector; fails when the
    /// projector cannot generate audio.
    pub fn load(model_path: &Path, projector_path: &Path, gpu: bool, threads: i32) -> Result<Self> {
        super::route_llama_logs();
        let model_c = CString::new(model_path.to_str().context("Model path is not UTF-8")?)?;
        let projector_c = CString::new(projector_path.to_str().context("Projector path is not UTF-8")?)?;

        // SAFETY: default-parameter constructors take no handles.
        let mut model_params = unsafe { llama_cpp::llama_model_default_params() };
        model_params.n_gpu_layers = if gpu { 999 } else { 0 };
        // SAFETY: valid C string; the returned model is owned below.
        let model = NonNull::new(unsafe { llama_cpp::llama_model_load_from_file(model_c.as_ptr(), model_params) })
            .context("Speech model load failed; check the file and memory")?;

        // SAFETY: default-parameter constructor.
        let mut context_params = unsafe { llama_cpp::llama_context_default_params() };
        context_params.n_ctx = CONTEXT;
        context_params.n_batch = BATCH as u32;
        context_params.n_ubatch = BATCH as u32;
        context_params.n_threads = threads.max(1);
        context_params.n_threads_batch = threads.max(1);
        // The audio decoder reads the backbone's hidden state after each step
        context_params.embeddings = 1;
        // SAFETY: model is live; the context is freed in Drop or on the error paths below.
        let Some(context) = NonNull::new(unsafe { llama_cpp::llama_init_from_model(model.as_ptr(), context_params) }) else {
            // SAFETY: model is owned here and not used after.
            unsafe { llama_cpp::llama_model_free(model.as_ptr()) };
            bail!("Speech context could not be created");
        };

        // SAFETY: default-parameter constructor.
        let mut projector_params = unsafe { mtmd::mtmd_context_params_default() };
        projector_params.use_gpu = gpu;
        projector_params.n_threads = threads.max(1);
        projector_params.warmup = false;
        // SAFETY: valid C string and live model.
        let projector = NonNull::new(unsafe {
            mtmd::mtmd_init_from_file(projector_c.as_ptr(), model.as_ptr().cast(), projector_params)
        });
        let owner = |projector| Self { model, context, projector, sample_rate: 0 };
        let Some(projector) = projector else {
            // SAFETY: handles owned here and not used after.
            unsafe {
                llama_cpp::llama_free(context.as_ptr());
                llama_cpp::llama_model_free(model.as_ptr());
            }
            bail!("Speech projector load failed; check that it belongs to this model");
        };
        let mut speech = owner(projector);
        // SAFETY: live projector.
        let info = unsafe { mtmd::mtmd_gen_audio_get_info(projector.as_ptr()) };
        if info.type_ == mtmd::MTMD_GEN_AUDIO_TYPE_NONE || info.sample_rate <= 0 {
            bail!("This projector does not generate audio");
        }
        speech.sample_rate = info.sample_rate as u32;
        Ok(speech)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Speaks `text`; returns a 16-bit mono WAV.
    pub fn speak(&mut self, text: &str, options: &SpeechOptions) -> Result<Vec<u8>> {
        if text.trim().is_empty() {
            bail!("Speech requires non-empty text");
        }
        let ctx = self.context.as_ptr();
        // A fresh utterance: nothing of the previous one stays in the cache
        // SAFETY: live context.
        unsafe { llama_cpp::llama_memory_clear(llama_cpp::llama_get_memory(ctx), true) };

        let generator = Generator::new(ctx, self.projector.as_ptr())?;
        let text_c = CString::new(text).context("Text contains a NUL character")?;
        let language_c = options.language.as_deref().map(CString::new).transpose()?;
        let input = mtmd::mtmd_helper_gen_audio_inp {
            seq_id: 0,
            prompt: text_c.as_ptr(),
            prompt_len: text.len(),
            speaker_ref: std::ptr::null_mut(),
            lang: language_c.as_ref().map_or(std::ptr::null(), |l| l.as_ptr()),
            top_k: TOP_K,
            top_p: TOP_P,
            seed: options.seed.unwrap_or(u32::MAX),
            out_type: mtmd::MTMD_HELPER_GEN_AUDIO_OUTTYPE_WAV,
        };
        // SAFETY: the input's strings outlive the generation (they live until this call returns).
        if unsafe { mtmd::mtmd_helper_gen_audio_set_input(generator.0.as_ptr(), &input) } != 0 {
            bail!("Speech input was refused (text or language not supported by this model)");
        }
        loop {
            // SAFETY: live generator.
            match unsafe { mtmd::mtmd_helper_gen_audio_step_prompt(generator.0.as_ptr(), BATCH) } {
                left if left < 0 => bail!("Speech prompt processing failed"),
                0 => break,
                _ => {}
            }
        }

        let sampler = Sampler::new(options.seed.unwrap_or(u32::MAX))?;
        // About 12.5 frames per second of speech; a generous bound on a text's length
        let max_frames = options.max_frames.unwrap_or(64 + text.chars().count() * 4);
        let mut sampled = sampler.sample(ctx);
        // SAFETY: live context with embeddings on; the pointer is read before the next step.
        let mut hidden: *const f32 = unsafe { llama_cpp::llama_get_embeddings_ith(ctx, -1) };
        let mut frames = 0usize;
        let mut stop = false;
        while !stop && frames < max_frames {
            let mut next: *const f32 = std::ptr::null();
            // SAFETY: hidden comes from the context or the previous step and is valid until this call.
            if unsafe { mtmd::mtmd_helper_gen_audio_step_gen(generator.0.as_ptr(), sampled, hidden, &mut next, &mut stop) } != 0 {
                bail!("Speech generation failed at frame {frames}");
            }
            if next.is_null() {
                break;
            }
            frames += 1;
            hidden = next;
            sampled = sampler.sample(ctx);
        }

        let (mut rate, mut data, mut len, mut samples) = (0i32, std::ptr::null(), 0usize, 0i64);
        // SAFETY: live generator; data stays valid until the generator is freed (below, after the copy).
        if unsafe { mtmd::mtmd_helper_gen_audio_get_output(generator.0.as_ptr(), &mut rate, &mut data, &mut len, &mut samples) } != 0
            || data.is_null()
            || len == 0
        {
            bail!("Speech produced no audio");
        }
        // SAFETY: data/len describe the generator's output buffer.
        let wav = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec();
        tracing::debug!("🗣️ [Speech] {frames} frames, {samples} samples at {rate} Hz");
        Ok(wav)
    }
}

impl Drop for SpeechModel {
    fn drop(&mut self) {
        // SAFETY: each handle is owned once and freed in reverse order of creation.
        unsafe {
            mtmd::mtmd_free(self.projector.as_ptr());
            llama_cpp::llama_free(self.context.as_ptr());
            llama_cpp::llama_model_free(self.model.as_ptr());
        }
    }
}

/// One utterance's generator, freed when dropped.
struct Generator(NonNull<mtmd::mtmd_helper_gen_audio>);

impl Generator {
    fn new(ctx: *mut c_void, projector: *mut mtmd::mtmd_context) -> Result<Self> {
        // SAFETY: live context and projector of the same model.
        NonNull::new(unsafe { mtmd::mtmd_helper_gen_audio_init(ctx, projector) })
            .map(Self)
            .context("Speech generator could not be created")
    }
}

impl Drop for Generator {
    fn drop(&mut self) {
        // SAFETY: owned generator.
        unsafe { mtmd::mtmd_helper_gen_audio_free(self.0.as_ptr()) }
    }
}

/// The backbone's code sampler: top-k, top-p, min-p, temperature, then a seeded draw.
struct Sampler(NonNull<c_void>);

impl Sampler {
    fn new(seed: u32) -> Result<Self> {
        // SAFETY: the chain takes ownership of each sampler added to it.
        unsafe {
            let chain = NonNull::new(llama_cpp::llama_sampler_chain_init(llama_cpp::LlamaSamplerChainParams { no_perf: true }))
                .context("Sampler could not be created")?;
            llama_cpp::llama_sampler_chain_add(chain.as_ptr(), llama_cpp::llama_sampler_init_top_k(TOP_K));
            llama_cpp::llama_sampler_chain_add(chain.as_ptr(), llama_cpp::llama_sampler_init_top_p(TOP_P, 1));
            llama_cpp::llama_sampler_chain_add(chain.as_ptr(), llama_cpp::llama_sampler_init_min_p(MIN_P, 1));
            llama_cpp::llama_sampler_chain_add(chain.as_ptr(), llama_cpp::llama_sampler_init_temp(TEMPERATURE));
            llama_cpp::llama_sampler_chain_add(chain.as_ptr(), llama_cpp::llama_sampler_init_dist(seed));
            Ok(Self(chain))
        }
    }

    fn sample(&self, ctx: *mut c_void) -> i32 {
        // SAFETY: live sampler and context; the token is accepted as tools/tts does.
        unsafe {
            let token = llama_cpp::llama_sampler_sample(self.0.as_ptr(), ctx, -1);
            llama_cpp::llama_sampler_accept(self.0.as_ptr(), token);
            token
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        // SAFETY: owned chain.
        unsafe { llama_cpp::llama_sampler_free(self.0.as_ptr()) }
    }
}
