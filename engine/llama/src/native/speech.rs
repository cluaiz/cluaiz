//! Speech from llama.cpp: a text-to-speech backbone GGUF and its audio projector (mmproj) make a
//! WAV with libmtmd's audio generation, the loop of llama.cpp's `tools/tts`. Upstream marks this
//! API experimental, so every call into it is in this module (and its declarations in `ffi::mtmd`).
//!
//! The backbone runs in its own model and context, separate from the chat model, so speaking never
//! touches a chat's state.

use crate::ffi::llama_cpp;
use crate::ffi::mtmd;
use anyhow::{bail, Context, Result};
use cluaiz_core::{media::DecodedAudio, media_limits::MediaLimits};
use std::ffi::{c_void, CStr, CString};
use std::path::Path;
use std::ptr::NonNull;
use std::time::Instant;

/// Sampling of the backbone's speech codes; the values of llama.cpp's defaults, which `tools/tts`
/// uses.
const TOP_K: i32 = 40;
const TOP_P: f32 = 0.95;
const MIN_P: f32 = 0.05;
const TEMPERATURE: f32 = 0.8;
/// Prompt tokens evaluated per step
const BATCH: i32 = 512;
/// Consumer polling cadence in generated frames; independent of the native codec's own batching.
const PCM_POLL_FRAMES: usize = 16;
/// Context for the prompt and the generated frames
const CONTEXT: u32 = 4096;

/// Placement is selected by the runtime's shared live-memory policy, not by model names.
#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
pub struct SpeechPlacement {
    pub n_gpu_layers: i32,
    pub threads: i32,
    pub threads_batch: i32,
    pub use_mmap: bool,
}

/// Measured successful utterance stages. Output time includes consumer backpressure.
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct SpeechTimings {
    pub preparation_seconds: f64,
    pub generation_seconds: f64,
    pub decoder_seconds: f64,
    pub output_seconds: f64,
    pub total_seconds: f64,
    pub frames: usize,
    pub native_samples: usize,
}

/// Backbone context/logits estimate; codec workspace is supplied by the existing media policy.
pub fn backbone_workspace_bytes(path: &Path) -> Result<u64> {
    let geometry = cluaiz_models::GgufProber::probe_geometry(path)?;
    geometry.kv_cache_bytes(CONTEXT, cluaiz_core::KvCacheQuantization::Kv16, BATCH as u32)
        .checked_add(geometry.logits_buffer_bytes(BATCH as u32)).context("Speech workspace overflow")
}

/// A loaded speech backbone with its audio projector.
pub struct SpeechModel {
    model: NonNull<c_void>,
    context: NonNull<c_void>,
    projector: NonNull<mtmd::mtmd_context>,
    sample_rate: u32,
    languages: Vec<String>,
    timings: SpeechTimings,
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
    /// Already decoded/authorized audio; the native speaker encoder conditions this utterance.
    pub speaker_reference: Option<DecodedAudio>,
    pub limits: MediaLimits,
    /// Output speaking rate; shared streaming tempo changes duration without changing sample rate.
    pub speed: Option<f32>,
}

impl SpeechModel {
    /// Loads the backbone (all layers on the GPU when `gpu`) and the projector; fails when the
    /// projector cannot generate audio.
    pub fn load(model_path: &Path, projector_path: &Path, gpu: bool, threads: i32) -> Result<Self> {
        Self::load_with_placement(model_path, projector_path, SpeechPlacement {
            n_gpu_layers: if gpu { -1 } else { 0 }, threads, threads_batch: threads, use_mmap: true,
        })
    }

    pub fn load_with_placement(model_path: &Path, projector_path: &Path, placement: SpeechPlacement) -> Result<Self> {
        super::route_llama_logs();
        let model_c = CString::new(model_path.to_str().context("Model path is not UTF-8")?)?;
        let projector_c = CString::new(projector_path.to_str().context("Projector path is not UTF-8")?)?;

        // SAFETY: default-parameter constructors take no handles.
        let mut model_params = unsafe { llama_cpp::llama_model_default_params() };
        model_params.n_gpu_layers = placement.n_gpu_layers;
        model_params.set_mmap(placement.use_mmap);
        // SAFETY: valid C string; the returned model is owned below.
        let model = NonNull::new(unsafe { llama_cpp::llama_model_load_from_file(model_c.as_ptr(), model_params) })
            .context("Speech model load failed; check the file and memory")?;

        // SAFETY: default-parameter constructor.
        let mut context_params = unsafe { llama_cpp::llama_context_default_params() };
        context_params.n_ctx = CONTEXT;
        context_params.n_batch = BATCH as u32;
        context_params.n_ubatch = BATCH as u32;
        context_params.n_threads = placement.threads.max(1);
        context_params.n_threads_batch = placement.threads_batch.max(1);
        context_params.offload_kqv = u8::from(placement.n_gpu_layers != 0);
        context_params.op_offload = u8::from(placement.n_gpu_layers != 0);
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
        projector_params.use_gpu = placement.n_gpu_layers != 0;
        projector_params.n_threads = placement.threads.max(1);
        projector_params.warmup = false;
        // SAFETY: valid C string and live model.
        let projector = NonNull::new(unsafe {
            mtmd::mtmd_init_from_file(projector_c.as_ptr(), model.as_ptr().cast(), projector_params)
        });
        let owner = |projector| Self { model, context, projector, sample_rate: 0, languages: Vec::new(), timings: SpeechTimings::default() };
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
        // SAFETY: vocabulary and its returned token strings are borrowed from the owned live model.
        unsafe {
            let vocab = llama_cpp::llama_model_get_vocab(model.as_ptr());
            for token in 0..llama_cpp::llama_vocab_n_tokens(vocab) {
                let text = llama_cpp::llama_vocab_get_text(vocab, token);
                if !text.is_null() {
                    if let Ok(text) = CStr::from_ptr(text).to_str() {
                        if let Some(name) = cluaiz_models::GgufProber::speech_language_token(text) {
                            speech.languages.push(name.to_owned());
                        }
                    }
                }
            }
        }
        Ok(speech)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn timings(&self) -> SpeechTimings { self.timings }

    /// Speaks `text`; returns a 16-bit mono WAV.
    pub fn speak(&mut self, text: &str, options: &SpeechOptions) -> Result<Vec<u8>> {
        let mut samples = Vec::new();
        self.speak_pcm(text, options, |part, _| {
            let retained = samples.len().checked_add(part.len()).and_then(|n| n.checked_mul(8))
                .context("Speech output budget overflow")?;
            options.limits.check_decoded_bytes(u64::try_from(retained)?)?;
            samples.extend_from_slice(part);
            Ok(())
        }, || false)?;
        cluaiz_media::wav::encode_pcm16(&samples, self.sample_rate, options.limits)
    }

    /// Emits actual new PCM while generating. Callback/receiver failures and cancellation stop
    /// at prompt/frame boundaries; a single native step cannot be interrupted by this adapter.
    pub fn speak_pcm(
        &mut self, text: &str, options: &SpeechOptions,
        mut on_pcm: impl FnMut(&[f32], u32) -> Result<()>, cancelled: impl Fn() -> bool,
    ) -> Result<()> {
        self.timings = SpeechTimings::default();
        let started = Instant::now();
        let mut timings = SpeechTimings::default();
        if text.trim().is_empty() {
            bail!("Speech requires non-empty text");
        }
        if cancelled() { bail!("Speech request cancelled"); }
        let mut tempo = cluaiz_media::tempo::Tempo::new(self.sample_rate, options.speed.unwrap_or(1.0), options.limits)?;
        let ctx = self.context.as_ptr();
        // A fresh utterance: nothing of the previous one stays in the cache
        // SAFETY: live context.
        unsafe { llama_cpp::llama_memory_clear(llama_cpp::llama_get_memory(ctx), true) };

        let text_c = CString::new(text).context("Text contains a NUL character")?;
        let language = options.language.as_deref().map(|l| resolve_language(l, &self.languages)).transpose()?;
        let language_c = language.as_deref().map(CString::new).transpose()?;
        let speaker = options.speaker_reference.as_ref().map(|audio| {
            // SAFETY: owned live projector; query its input rate, not its output WAV rate.
            let (rate, supports_audio) = unsafe {
                (mtmd::mtmd_get_audio_sample_rate(self.projector.as_ptr()), mtmd::mtmd_support_audio(self.projector.as_ptr()))
            };
            if rate <= 0 || !supports_audio {
                bail!("This speech projector has no speaker-reference encoder");
            }
            super::media::Bitmap::from_audio(audio, rate as u32, options.limits).map(|(bitmap, _)| bitmap)
        }).transpose()?;
        // Declared after strings/bitmap so the generator is freed before its borrowed inputs.
        let generator = Generator::new(ctx, self.projector.as_ptr())?;
        let input = mtmd::mtmd_helper_gen_audio_inp {
            seq_id: 0,
            prompt: text_c.as_ptr(),
            prompt_len: text.len(),
            speaker_ref: speaker.as_ref().map_or(std::ptr::null_mut(), |b| b.0.as_ptr()),
            lang: language_c.as_ref().map_or(std::ptr::null(), |l| l.as_ptr()),
            top_k: TOP_K,
            top_p: TOP_P,
            seed: options.seed.unwrap_or(u32::MAX),
            out_type: mtmd::MTMD_HELPER_GEN_AUDIO_OUTTYPE_PCM,
        };
        // SAFETY: the input's strings outlive the generation (they live until this call returns).
        if unsafe { mtmd::mtmd_helper_gen_audio_set_input(generator.0.as_ptr(), &input) } != 0 {
            bail!("Speech input was refused (text or language not supported by this model)");
        }
        loop {
            if cancelled() { bail!("Speech request cancelled"); }
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
        if hidden.is_null() { bail!("Speech prompt produced no hidden state"); }
        let mut frames = 0usize;
        let mut stop = false;
        let mut emitted_native = 0;
        timings.preparation_seconds = started.elapsed().as_secs_f64();
        while !stop && frames < max_frames {
            if cancelled() { bail!("Speech request cancelled"); }
            let mut next: *const f32 = std::ptr::null();
            let frame_started = Instant::now();
            // SAFETY: hidden comes from the context or the previous step and is valid until this call.
            if unsafe { mtmd::mtmd_helper_gen_audio_step_gen(generator.0.as_ptr(), sampled, hidden, &mut next, &mut stop) } != 0 {
                bail!("Speech generation failed at frame {frames}");
            }
            timings.generation_seconds += frame_started.elapsed().as_secs_f64();
            if next.is_null() {
                break;
            }
            frames += 1;
            hidden = next;
            let sampler_started = Instant::now();
            sampled = sampler.sample(ctx);
            timings.generation_seconds += sampler_started.elapsed().as_secs_f64();
            if frames % PCM_POLL_FRAMES == 0 {
                generator.emit_delta(&mut emitted_native, self.sample_rate, options.limits, &mut tempo, &mut on_pcm, &mut timings)?;
            }
        }
        if !stop && frames >= max_frames { bail!("Speech reached its frame limit before completion; shorten the input"); }

        if cancelled() { bail!("Speech request cancelled"); }
        generator.emit_delta(&mut emitted_native, self.sample_rate, options.limits, &mut tempo, &mut on_pcm, &mut timings)?;
        if emitted_native == 0 { bail!("Speech produced no audio"); }
        let output_started = Instant::now();
        let tail = tempo.finish()?;
        if !tail.is_empty() { on_pcm(&tail, self.sample_rate)?; }
        timings.output_seconds += output_started.elapsed().as_secs_f64();
        timings.total_seconds = started.elapsed().as_secs_f64();
        timings.frames = frames;
        timings.native_samples = emitted_native;
        self.timings = timings;
        tracing::info!(?timings, sample_rate = self.sample_rate, "Native speech stage timings");
        Ok(())
    }
}

/// ISO codes for the codec language names found in the actual model vocabulary.
pub fn language_codes(supported: &[String]) -> Result<Vec<String>> {
    let data: serde_json::Value = serde_json::from_str(include_str!("data/speech_languages.json"))?;
    let mut codes: Vec<_> = data["aliases"].as_object().context("Missing speech language aliases")?
        .iter().filter(|(_, name)| name.as_str().is_some_and(|name| supported.iter().any(|s| s == name)))
        .map(|(code, _)| code.clone()).collect();
    codes.sort();
    Ok(codes)
}

fn resolve_language(requested: &str, supported: &[String]) -> Result<String> {
    let code = requested.split(['-', '_']).next().unwrap_or(requested).to_ascii_lowercase();
    let data: serde_json::Value = serde_json::from_str(include_str!("data/speech_languages.json"))?;
    let name = data["aliases"][&code].as_str().unwrap_or(&code);
    if !supported.iter().any(|l| l == name) {
        bail!("Speech model does not support language '{requested}'. Supported languages: {}. Select a model that supports the requested language.", supported.join(", "));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_requested_language_uses_actual_vocabulary_not_an_assumed_model_list() -> Result<()> {
        assert_eq!(language_codes(&["english".into(), "spanish".into()])?, ["en", "es"]);
        assert!(language_codes(&[])?.is_empty());
        let data: serde_json::Value = serde_json::from_str(include_str!("data/speech_languages.json"))?;
        for case in data["tests"].as_array().context("Missing language cases")? {
            let requested = case["requested"].as_str().context("Missing request")?;
            let supported: Vec<String> = serde_json::from_value(case["supported"].clone())?;
            let result = resolve_language(requested, &supported);
            if case["error"].as_bool().unwrap_or(false) {
                assert!(result.unwrap_err().to_string().contains(requested));
            } else { assert_eq!(result?, case["expected"].as_str().context("Missing expected language")?); }
        }
        Ok(())
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

    fn emit_delta(
        &self, emitted: &mut usize, expected_rate: u32, limits: MediaLimits,
        tempo: &mut cluaiz_media::tempo::Tempo, on_pcm: &mut impl FnMut(&[f32], u32) -> Result<()>,
        timings: &mut SpeechTimings,
    ) -> Result<()> {
        let (mut rate, mut data, mut len, mut samples) = (0i32, std::ptr::null(), 0usize, 0i64);
        let decoder_started = Instant::now();
        // SAFETY: owned live generator in PCM mode; the returned buffer is borrowed only below,
        // before another generation/decoder call. get_output retains code2wav state across flushes.
        if unsafe { mtmd::mtmd_helper_gen_audio_get_output(self.0.as_ptr(), &mut rate, &mut data, &mut len, &mut samples) } != 0 {
            bail!("Speech PCM output failed");
        }
        timings.decoder_seconds += decoder_started.elapsed().as_secs_f64();
        let count = usize::try_from(samples).context("Invalid native speech sample count")?;
        if rate <= 0 || rate as u32 != expected_rate || count < *emitted
            || count.checked_mul(std::mem::size_of::<f32>()) != Some(len) {
            bail!("Native speech PCM layout/rate changed during generation");
        }
        if count == *emitted { return Ok(()); }
        if data.is_null() || (data as usize) % std::mem::align_of::<f32>() != 0 {
            bail!("Native speech PCM buffer is invalid");
        }
        limits.check_decoded_bytes(u64::try_from(len.checked_mul(2).context("Speech PCM budget overflow")?)?)?;
        // SAFETY: native PCM mode supplies count aligned F32 values, verified against byte length;
        // its lifetime ends before this method returns and no native generator call occurs meanwhile.
        let pcm = unsafe { std::slice::from_raw_parts(data.cast::<f32>(), count) };
        let output_started = Instant::now();
        let output = tempo.write(&pcm[*emitted..])?;
        *emitted = count;
        if !output.is_empty() { on_pcm(&output, expected_rate)?; }
        timings.output_seconds += output_started.elapsed().as_secs_f64();
        Ok(())
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
