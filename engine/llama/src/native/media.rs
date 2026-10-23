//! Projector RAII. Only the paired session may construct this native owner.

use super::core::NativeLlama;
use crate::ffi::{llama_cpp, mtmd};
use anyhow::{bail, Context, Result};
use cluaiz_core::{
    chat::{ChatEnvelope, ChatMessage},
    media::{DecodedAudio, PreparedPart},
    media_limits::MediaLimits,
    processing::{
        AttachmentRef, ProcessingReporter, ProcessingStage as Stage, ProcessingState as State,
        ProcessingWork,
    },
};
use cluaiz_media::resample::{prepare_mono, resample_linear, PeakNormalization};
use cluaiz_plan::media_budget::ProjectorPlacement;
use std::{
    cell::Cell,
    ffi::{c_void, CStr, CString},
    marker::PhantomData,
    num::NonZeroU32,
    ptr::NonNull,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

/// Actual encoder queries after native initialization; not API route availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectorSupport {
    pub vision: bool,
    pub audio_sample_rate: Option<NonZeroU32>,
}

pub(crate) struct Projector {
    ptr: NonNull<mtmd::mtmd_context>,
    // Native progress callbacks may retain user_data until projector destruction.
    _interrupt: Arc<AtomicBool>,
    support: ProjectorSupport,
    // Encoder evaluation is not thread-safe; ownership may move, but cannot be shared.
    _not_sync: PhantomData<Cell<()>>,
}

// SAFETY: no thread-affine Rust references; native calls are serialized by the owning
// session. Cell keeps the owner !Sync, and Drop runs before the paired llama is freed.
unsafe impl Send for Projector {}

// SAFETY: the native loader must provide null or our retained Arc<AtomicBool> pointer.
unsafe extern "C" fn load_progress(_progress: f32, user_data: *mut c_void) -> bool {
    if user_data.is_null() {
        return false;
    }
    // SAFETY: init supplies Arc::as_ptr; Projector retains that Arc for the native lifetime.
    !unsafe { &*user_data.cast::<AtomicBool>() }.load(Ordering::Acquire)
}

impl Projector {
    pub(crate) fn load(
        llama: &NativeLlama,
        path: &CString,
        placement: ProjectorPlacement,
        threads: i32,
    ) -> Result<Self> {
        if llama.model_ptr.is_null() || threads <= 0 {
            bail!("Projector requires a loaded model and a positive thread count");
        }
        let interrupt = Arc::clone(&llama.interrupt_signal);
        // SAFETY: no handles required by this native default-parameter constructor.
        let mut params = unsafe { mtmd::mtmd_context_params_default() };
        params.use_gpu = placement == ProjectorPlacement::Gpu;
        params.n_threads = threads;
        // Disable the optional encode warmup; native initialization still owns its setup allocations.
        params.warmup = false;
        params.progress_callback = Some(load_progress);
        params.progress_callback_user_data = Arc::as_ptr(&interrupt).cast_mut().cast();
        // SAFETY: valid CString/model owned by the session; retained Arc keeps callback
        // user_data live. Native initialization checks projector embedding dimensions.
        let ptr = NonNull::new(unsafe {
            mtmd::mtmd_init_from_file(path.as_ptr(), llama.model_ptr.cast(), params)
        });
        let ptr = match ptr {
            Some(ptr) => ptr,
            None if interrupt.load(Ordering::Acquire) => bail!("Projector load cancelled"),
            None => bail!("Native projector load failed; check pairing, file and memory"),
        };
        let mut owner = Self {
            ptr,
            _interrupt: interrupt,
            support: ProjectorSupport {
                vision: false,
                audio_sample_rate: None,
            },
            _not_sync: PhantomData,
        };
        // SAFETY: this owner retains the successfully initialized native context.
        let (vision, audio, rate) = unsafe {
            (
                mtmd::mtmd_support_vision(ptr.as_ptr()),
                mtmd::mtmd_support_audio(ptr.as_ptr()),
                mtmd::mtmd_get_audio_sample_rate(ptr.as_ptr()),
            )
        };
        let audio_sample_rate = if audio {
            Some(
                NonZeroU32::new(u32::try_from(rate)?)
                    .context("Encoder reported a zero audio rate")?,
            )
        } else {
            None
        };
        if !vision && audio_sample_rate.is_none() {
            bail!("Projector has no supported input encoder");
        }
        if owner._interrupt.load(Ordering::Acquire) {
            bail!("Projector load cancelled");
        }
        owner.support = ProjectorSupport {
            vision,
            audio_sample_rate,
        };
        Ok(owner)
    }

    pub(crate) fn support(&self) -> ProjectorSupport {
        self.support
    }

    #[cfg(test)]
    pub(crate) fn test_address(&self) -> usize {
        self.ptr.as_ptr() as usize
    }
}

impl Drop for Projector {
    fn drop(&mut self) {
        // SAFETY: one native owner; the paired model and callback Arc are still live.
        unsafe { mtmd::mtmd_free(self.ptr.as_ptr()) }
    }
}

pub(super) struct Bitmap(pub(super) NonNull<mtmd::mtmd_bitmap>);

impl Bitmap {
    /// Shared mono/rate preparation and copying native owner for chat or speaker audio.
    pub(super) fn from_audio(audio: &DecodedAudio, rate: u32, limits: MediaLimits) -> Result<(Self, DecodedAudio)> {
        let original = u64::try_from(std::mem::size_of_val(audio.samples()))?;
        let mono = prepare_mono(audio, PeakNormalization::Preserve, limits)?;
        let extra = if mono.samples().as_ptr() == audio.samples().as_ptr() { 0 } else { original };
        let remaining = limits.max_decoded_bytes().checked_sub(extra).filter(|n| *n > 0)
            .context("No budget remains for audio resampling")?;
        let prepared = resample_linear(&mono, rate, MediaLimits::new(limits.max_input_bytes(), remaining)?)?;
        let bytes = u64::try_from(std::mem::size_of_val(prepared.samples()))?;
        let peak = original.checked_add(u64::try_from(std::mem::size_of_val(mono.samples()))?)
            .and_then(|n| bytes.checked_mul(2).and_then(|copy| n.checked_add(copy)))
            .context("PCM copying budget overflow")?;
        limits.check_decoded_bytes(peak)?;
        u32::try_from(prepared.samples().len()).context("Audio is too long for native bitmap")?;
        // SAFETY: validated finite mono PCM at the queried native rate; constructor copies it.
        let ptr = NonNull::new(unsafe {
            mtmd::mtmd_bitmap_init_from_audio(prepared.samples().len(), prepared.samples().as_ptr())
        }).context("Native audio allocation failed")?;
        Ok((Self(ptr), prepared))
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: exactly one owner of the native copying constructor's result.
        unsafe { mtmd::mtmd_bitmap_free(self.0.as_ptr()) }
    }
}

struct Chunks(NonNull<mtmd::mtmd_input_chunks>);

impl Drop for Chunks {
    fn drop(&mut self) {
        // SAFETY: chunks and their items were allocated by mtmd and have one owner.
        unsafe { mtmd::mtmd_input_chunks_free(self.0.as_ptr()) }
    }
}

static REQUEST_MARKER_ID: AtomicU64 = AtomicU64::new(0);

/// One piece of what the KV cache holds after a media generation, in prompt order (llama-server
/// keeps the same in `server_tokens`): text tokens, or an encoded image/audio known by its id.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum CachedChunk {
    Text(Vec<i32>),
    Media { id: String, n_pos: i32 },
}

/// Where a new prompt stops matching the cache, as llama-server's `get_common_prefix` with media:
/// text compares token by token, media only as a whole chunk with the same id. Returns the KV
/// positions to keep, the first prompt chunk to evaluate and its text tokens already in the cache.
/// At least one prompt token is always left to evaluate, so the sampler gets fresh logits.
pub(crate) fn cache_prefix(cached: &[CachedChunk], prompt: &[CachedChunk]) -> (i32, usize, usize) {
    let mut pos = 0;
    for (i, chunk) in prompt.iter().enumerate() {
        match (cached.get(i), chunk) {
            (Some(CachedChunk::Text(old)), CachedChunk::Text(new)) => {
                let k = old.iter().zip(new).take_while(|(a, b)| a == b).count();
                if k == new.len() && i + 1 == prompt.len() {
                    // The whole prompt is cached: evaluate its last token again
                    let back = usize::from(k > 0);
                    return (pos + (k - back) as i32, i, k - back);
                }
                pos += k as i32;
                if k < new.len() {
                    return (pos, i, k);
                }
                if k < old.len() {
                    return (pos, i + 1, 0);
                }
            }
            (
                Some(CachedChunk::Media { id: a, n_pos: pa }),
                CachedChunk::Media { id: b, n_pos: pb },
            ) if a == b && pa == pb && i + 1 < prompt.len() => pos += pb,
            _ => return (pos, i, 0),
        }
    }
    (pos, prompt.len(), 0)
}

/// Content identity of a media part (FNV-1a over its shape and samples, as llama-server hashes
/// bitmaps), so the same attachment in a later turn is recognised in the KV cache.
fn content_hash(header: &[u64], data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in header
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .chain(data.iter().copied())
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Native buffers stay owned until evaluation/generation completes. These unique
/// template placeholders never reach tokenization; typed bitmap parts replace them.
pub(crate) struct MediaInput {
    bitmaps: Vec<Bitmap>,
    /// Content hash of each bitmap (its native id is "index:hash")
    hashes: Vec<u64>,
    markers: Vec<String>,
    targets: Vec<(usize, usize)>,
    pub(crate) progress: Option<ProcessingReporter>,
    /// KV layout left by the previous media generation (set by the session before evaluation)
    pub(crate) cached: Vec<CachedChunk>,
    /// This prompt's KV layout and its text token count, known after evaluation
    layout: Vec<CachedChunk>,
    pub(crate) prompt_text_tokens: usize,
}

pub(crate) struct MediaPrefill {
    pub(crate) text_tokens: Vec<i32>,
    pub(crate) positions: i32,
    pub(crate) context: super::core::ContextReport,
}

/// Locate each placeholder exactly once and in order; template loss/duplication is an error.
fn marker_ranges(prompt: &str, markers: &[String]) -> Result<Vec<(usize, usize)>> {
    let mut cursor = 0;
    let mut ranges = Vec::with_capacity(markers.len());
    for marker in markers {
        let mut occurrences = prompt.match_indices(marker);
        let (start, _) = occurrences
            .next()
            .context("Template removed a media part")?;
        if occurrences.next().is_some() || start < cursor {
            bail!("Template duplicated or reordered a media part");
        }
        cursor = start + marker.len();
        ranges.push((start, cursor));
    }
    Ok(ranges)
}

impl MediaInput {
    /// Conservative conversion peak derived from this request's geometry and queried audio rate.
    /// The API may retain every original Arc throughout generation/tool continuations.
    pub(crate) fn additional_bytes(
        request: &ChatEnvelope<Vec<PreparedPart>>,
        support: ProjectorSupport,
        limits: MediaLimits,
    ) -> Result<u64> {
        let retained = PreparedPart::request_bytes(&request.messages)?;
        let mut copies = 0_u64;
        let mut extra_peak = 0_u64;
        for part in request.messages.iter().flat_map(|m| &m.content) {
            let (copy, scratch) = match part {
                PreparedPart::Text { .. } => continue,
                PreparedPart::Image { image } => (u64::try_from(image.pixels().len())?, 0),
                PreparedPart::Audio { audio } => {
                    let rate = support
                        .audio_sample_rate
                        .context("Loaded projector does not support audio input")?;
                    let frames =
                        u64::try_from(audio.samples().len() / usize::from(audio.channels()))?;
                    let output = frames
                        .checked_mul(u64::from(rate.get()))
                        .context("Resampled frame count overflow")?
                        / u64::from(audio.sample_rate());
                    if output == 0 {
                        bail!("Audio is too short for the requested sample rate");
                    }
                    let sample_bytes = std::mem::size_of::<f32>() as u64;
                    let bytes = output
                        .checked_mul(sample_bytes)
                        .context("PCM byte count overflow")?;
                    // Include mono/resampling temporaries and Vec-to-Arc copies conservatively.
                    let scratch = frames
                        .checked_mul(sample_bytes * 2)
                        .and_then(|n| n.checked_add(bytes))
                        .context("PCM workspace overflow")?;
                    (bytes, scratch)
                }
            };
            copies = copies
                .checked_add(copy)
                .context("Native copy byte overflow")?;
            extra_peak = extra_peak.max(
                copies
                    .checked_add(scratch)
                    .context("Media workspace overflow")?,
            );
        }
        limits.check_decoded_bytes(
            retained
                .checked_add(extra_peak)
                .context("Media conversion peak overflow")?,
        )?;
        Ok(extra_peak)
    }

    pub(crate) fn prepare(
        request: ChatEnvelope<Vec<PreparedPart>>,
        support: ProjectorSupport,
        limits: MediaLimits,
    ) -> Result<(ChatEnvelope, Self)> {
        let retained = PreparedPart::request_bytes(&request.messages)?;
        Self::additional_bytes(&request, support, limits)?;
        let prefix = loop {
            let candidate = format!(
                "[cluaiz-media-{}-",
                REQUEST_MARKER_ID.fetch_add(1, Ordering::Relaxed)
            );
            if !request.messages.iter().any(|m| {
                m.content.iter().any(
                    |part| matches!(part, PreparedPart::Text { text } if text.contains(&candidate)),
                )
            }) {
                break candidate;
            }
        };
        let mut input = Self {
            bitmaps: Vec::new(),
            hashes: Vec::new(),
            markers: Vec::new(),
            targets: Vec::new(),
            progress: None,
            cached: Vec::new(),
            layout: Vec::new(),
            prompt_text_tokens: 0,
        };
        let mut native_bytes = 0_u64;
        let mut messages = Vec::with_capacity(request.messages.len());
        for (message_index, message) in request.messages.into_iter().enumerate() {
            if message.role.contains('\0') {
                bail!("Message role contains NUL");
            }
            let mut content = String::new();
            for (part_index, part) in message.content.into_iter().enumerate() {
                let part_bytes = u64::try_from(part.retained_bytes())?;
                let other = retained
                    .checked_sub(part_bytes)
                    .context("Media byte accounting underflow")?;
                let used = other
                    .checked_add(native_bytes)
                    .context("Native media byte overflow")?;
                let remaining = limits
                    .max_decoded_bytes()
                    .checked_sub(used)
                    .filter(|n| *n > 0)
                    .context("No decoded budget remains for media conversion")?;
                let local_limits = MediaLimits::new(limits.max_input_bytes(), remaining)?;
                let (bitmap, bytes, hash) = match part {
                    PreparedPart::Text { text } => {
                        if text.contains('\0') {
                            bail!("Message text contains NUL");
                        }
                        content.push_str(&text);
                        continue;
                    }
                    PreparedPart::Image { image } => {
                        if !support.vision {
                            bail!("Loaded projector does not support image input");
                        }
                        if image.width() > i32::MAX as u32 || image.height() > i32::MAX as u32 {
                            bail!("Image dimensions exceed native preprocessing range");
                        }
                        let bytes = u64::try_from(image.pixels().len())?;
                        local_limits.check_decoded_bytes(
                            bytes
                                .checked_mul(2)
                                .context("RGB copying budget overflow")?,
                        )?;
                        // SAFETY: validated exact RGB layout and checked source+native copy budget.
                        let bitmap = NonNull::new(unsafe {
                            mtmd::mtmd_bitmap_init(
                                image.width(),
                                image.height(),
                                image.pixels().as_ptr(),
                            )
                        })
                        .context("Native image allocation failed")?;
                        let hash = content_hash(
                            &[u64::from(image.width()), u64::from(image.height())],
                            image.pixels(),
                        );
                        (Bitmap(bitmap), bytes, hash)
                    }
                    PreparedPart::Audio { audio } => {
                        let rate = support
                            .audio_sample_rate
                            .context("Loaded projector does not support audio input")?;
                        let (bitmap, prepared) = Bitmap::from_audio(&audio, rate.get(), local_limits)?;
                        let bytes = u64::try_from(std::mem::size_of_val(prepared.samples()))?;
                        let samples: Vec<u8> = prepared
                            .samples()
                            .iter()
                            .flat_map(|v| v.to_le_bytes())
                            .collect();
                        let hash = content_hash(&[u64::from(rate.get())], &samples);
                        (bitmap, bytes, hash)
                    }
                };
                native_bytes = native_bytes
                    .checked_add(bytes)
                    .context("Native bitmap budget overflow")?;
                let marker = format!("{}{index}]", prefix, index = input.bitmaps.len());
                let id = CString::new(format!("{}:{hash:016x}", input.bitmaps.len()))?;
                // SAFETY: constructor copies this ID; bitmap remains exclusively owned.
                unsafe {
                    mtmd::mtmd_bitmap_set_id(bitmap.0.as_ptr(), id.as_ptr());
                }
                content.push_str(&marker);
                input.markers.push(marker);
                input.targets.push((message_index, part_index));
                input.bitmaps.push(bitmap);
                input.hashes.push(hash);
            }
            messages.push(ChatMessage {
                role: message.role,
                content,
            });
        }
        if input.bitmaps.is_empty() {
            bail!("Media generation requires an image or audio part");
        }
        Ok((
            ChatEnvelope {
                messages,
                samplers: request.samplers,
                think_mode: request.think_mode,
                reasoning_effort: request.reasoning_effort,
                reasoning_budget_message: request.reasoning_budget_message,
                chat_template_kwargs: request.chat_template_kwargs,
                grammar: request.grammar,
                response_format: request.response_format,
            },
            input,
        ))
    }

    fn tokenize(
        &self,
        projector: &Projector,
        llama: &NativeLlama,
        prompt: &str,
    ) -> Result<(Chunks, usize, i32)> {
        let ranges = marker_ranges(prompt, &self.markers)?;
        let mut text = Vec::with_capacity(ranges.len() + 1);
        let mut cursor = 0;
        for &(start, end) in &ranges {
            text.push(CString::new(&prompt[cursor..start])?);
            cursor = end;
        }
        text.push(CString::new(&prompt[cursor..])?);
        let texts: Vec<mtmd::mtmd_input_text> = text
            .iter()
            .map(|s| mtmd::mtmd_input_text {
                text: s.as_ptr(),
                text_len: s.as_bytes().len(),
                add_special: false,
                parse_special: true,
            })
            .collect();
        let mut parts = Vec::with_capacity(texts.len() + self.bitmaps.len());
        for (index, t) in texts.iter().enumerate() {
            if t.text_len > 0 {
                parts.push(mtmd::mtmd_input_part {
                    text: t,
                    bitmap: std::ptr::null(),
                });
            }
            if let Some(bitmap) = self.bitmaps.get(index) {
                parts.push(mtmd::mtmd_input_part {
                    text: std::ptr::null(),
                    bitmap: bitmap.0.as_ptr(),
                });
            }
        }
        let pointers: Vec<*const mtmd::mtmd_input_part> =
            parts.iter().map(|part| part as *const _).collect();
        // SAFETY: exclusive live model/projector; text/parts/bitmaps stay alive during tokenization/evaluation.
        unsafe {
            let chunks = Chunks(
                NonNull::new(mtmd::mtmd_input_chunks_init())
                    .context("Native chunks allocation failed")?,
            );
            let vocab = llama_cpp::llama_model_get_vocab(llama.model_ptr);
            let code = mtmd::mtmd_tokenize_from_parts(
                projector.ptr.as_ptr(),
                chunks.0.as_ptr(),
                pointers.as_ptr(),
                pointers.len(),
                llama_cpp::llama_vocab_get_add_bos(vocab),
            );
            if code != 0 {
                bail!("Native media tokenization failed with code {code}");
            }
            let count = mtmd::mtmd_helper_get_n_tokens(chunks.0.as_ptr());
            let positions = mtmd::mtmd_helper_get_n_pos(chunks.0.as_ptr());
            Ok((chunks, count, positions))
        }
    }

    fn remove_history(
        &mut self,
        messages: &mut Vec<(String, String)>,
        range: std::ops::Range<usize>,
    ) -> Result<()> {
        let current = messages
            .iter()
            .rposition(|(role, _)| role.eq_ignore_ascii_case("user"))
            .context("Media request has no current user turn")?;
        anyhow::ensure!(
            range.end <= current,
            "Cannot discard the current media turn"
        );
        for index in (0..self.markers.len()).rev() {
            if messages[range.clone()]
                .iter()
                .any(|(_, text)| text.contains(&self.markers[index]))
            {
                self.bitmaps.remove(index);
                self.hashes.remove(index);
                self.markers.remove(index);
                self.targets.remove(index);
            }
        }
        messages.drain(range);
        for (index, (bitmap, hash)) in self.bitmaps.iter().zip(&self.hashes).enumerate() {
            let id = CString::new(format!("{index}:{hash:016x}"))?;
            // SAFETY: live exclusively owned bitmap; native setter copies the supplied ID.
            unsafe {
                mtmd::mtmd_bitmap_set_id(bitmap.0.as_ptr(), id.as_ptr());
            }
        }
        Ok(())
    }

    /// The KV layout after generation: this prompt's chunks plus the answer tokens decoded after it.
    pub(crate) fn cached_after(&mut self, generated: &[i32]) -> Vec<CachedChunk> {
        let mut layout = std::mem::take(&mut self.layout);
        match layout.last_mut() {
            Some(CachedChunk::Text(tokens)) => tokens.extend_from_slice(generated),
            _ => layout.push(CachedChunk::Text(generated.to_vec())),
        }
        layout
    }

    pub(crate) fn evaluate(
        &mut self,
        projector: &mut Projector,
        llama: &mut NativeLlama,
        max_tokens: usize,
        messages: &mut Vec<(String, String)>,
        render: impl Fn(&NativeLlama, &[(String, String)]) -> Result<String>,
    ) -> Result<MediaPrefill> {
        if max_tokens == 0 || max_tokens > i32::MAX as usize {
            bail!("Invalid media output token budget");
        }
        if let Some(p) = &self.progress {
            p.emit(Stage::Prefill, State::Running, None, None);
        }
        let n_ctx = llama.n_ctx as usize;
        let mut report = super::core::ContextReport {
            n_ctx,
            stop: "eos",
            kv_reuse: "prefix",
            cause: "media",
            ..Default::default()
        };
        // Same answer room as text requests: max_tokens, at most a quarter of the context
        let reserve = cluaiz_core::answer_reserve(max_tokens, n_ctx);
        let capacity = n_ctx.saturating_sub(reserve);
        let shifting = cluaiz_core::Settings::current()
            .optimization
            .context_shifting;
        let (mut previous, mut to_free) = (None, 0_usize);
        let (chunks, count, positions) = loop {
            if llama.interrupt_signal.load(Ordering::Acquire)
                || self.progress.as_ref().is_some_and(|p| p.cancelled())
            {
                bail!("Media prefill cancelled");
            }
            let prompt = render(llama, messages)?;
            let (chunks, count, positions) = self.tokenize(projector, llama, &prompt)?;
            let occupied = count
                .max(usize::try_from(positions).context("Negative native media position count")?);
            if let Some(last) = previous {
                let freed = usize::saturating_sub(last, occupied);
                report.dropped_tokens += freed;
                to_free = to_free.saturating_sub(freed);
            }
            if occupied > 0 && occupied <= capacity && to_free == 0 {
                break (chunks, count, positions);
            }
            let target = cluaiz_core::tokens_to_free(
                shifting,
                n_ctx,
                occupied.saturating_sub(capacity),
                false,
            );
            let current = messages
                .iter()
                .rposition(|(role, _)| role.eq_ignore_ascii_case("user"))
                .unwrap_or(0);
            let sizes: Vec<_> = messages[..=current]
                .iter()
                .map(|(role, _)| (role.as_str(), 1))
                .collect();
            let (n_drop, _) = cluaiz_core::oldest_turns_to_drop(&sizes, 1);
            if occupied == 0 || target.is_none() || n_drop == 0 {
                if occupied > 0 && occupied <= capacity {
                    break (chunks, count, positions);
                }
                return Err(super::core::ContextExceeded(format!(
                    "request ({occupied} tokens) exceeds the available context size ({n_ctx} tokens, {reserve} kept for the answer). {}",
                    if n_drop == 0 {
                        "The current message and its attachments alone are too large: use a model with a larger context, or send a smaller document or image"
                    } else {
                        "Context shifting is Off: start a new chat or turn context shifting on"
                    }
                ))
                .into());
            }
            if to_free == 0 {
                to_free = target.unwrap_or(0);
            }
            previous = Some(occupied);
            drop(chunks);
            let start = usize::from(messages[0].0.eq_ignore_ascii_case("system"));
            self.remove_history(messages, start..start + n_drop)?;
            report.dropped_messages += n_drop;
            report.truncated = true;
        };
        // SAFETY: exclusive live native owners; final chunks retain all tokenized data.
        unsafe {
            let mut text_tokens = Vec::new();
            let mut layout = Vec::new();
            let mut chunk_targets = Vec::new();
            let mut slice_totals = vec![0_u64; self.targets.len()];
            let mut slice_done = vec![0_u64; self.targets.len()];
            for i in 0..mtmd::mtmd_input_chunks_size(chunks.0.as_ptr()) {
                let chunk = mtmd::mtmd_input_chunks_get(chunks.0.as_ptr(), i);
                if chunk.is_null() {
                    bail!("Native tokenizer returned a null chunk");
                }
                if mtmd::mtmd_input_chunk_get_type(chunk)
                    == mtmd::mtmd_input_chunk_type_MTMD_INPUT_CHUNK_TYPE_TEXT
                {
                    chunk_targets.push(None);
                    let mut len = 0;
                    let tokens = mtmd::mtmd_input_chunk_get_tokens_text(chunk, &mut len);
                    if len > 0 {
                        if tokens.is_null() {
                            bail!("Native text chunk has no token storage");
                        }
                        text_tokens.extend_from_slice(std::slice::from_raw_parts(tokens, len));
                    }
                    let tokens = if len > 0 {
                        std::slice::from_raw_parts(tokens, len).to_vec()
                    } else {
                        Vec::new()
                    };
                    layout.push(CachedChunk::Text(tokens));
                } else {
                    let id = mtmd::mtmd_input_chunk_get_id(chunk);
                    if id.is_null() {
                        bail!("Native media chunk lost attachment identity");
                    }
                    let id = CStr::from_ptr(id).to_str()?;
                    let index: usize = id
                        .split(':')
                        .next()
                        .context("Native media chunk id has no index")?
                        .parse()?;
                    layout.push(CachedChunk::Media {
                        id: id.to_owned(),
                        n_pos: mtmd::mtmd_input_chunk_get_n_pos(chunk),
                    });
                    let total = slice_totals
                        .get_mut(index)
                        .context("Native media attachment index is invalid")?;
                    *total += 1;
                    chunk_targets.push(Some(index));
                }
            }
            if llama.interrupt_signal.load(Ordering::Acquire) {
                bail!("Media prefill cancelled");
            }
            let memory = llama_cpp::llama_get_memory(llama.ctx_ptr);
            let chunk_count = mtmd::mtmd_input_chunks_size(chunks.0.as_ptr());
            // Reuse the KV of the previous media turn up to where this prompt first differs
            let (mut keep, mut first, mut skip) = cache_prefix(&self.cached, &layout);
            let swa = llama_cpp::llama_model_n_swa(llama.model_ptr) > 0;
            if keep > 0 {
                // A sliding-window cache keeps only the latest positions (llama-server's check): restart
                // at the newest checkpoint inside the kept prefix, else process everything again
                let n_swa = llama_cpp::llama_model_n_swa(llama.model_ptr).max(1);
                if llama_cpp::llama_memory_seq_pos_min(memory, 0) > (keep - n_swa).max(0) {
                    let ctx = llama.ctx_ptr;
                    (keep, first, skip) = llama
                        .checkpoints
                        .iter()
                        .rev()
                        .filter(|c| c.n_tokens > 0 && c.n_tokens <= keep)
                        .find_map(|c| text_position(&layout, c.n_tokens).map(|(i, o)| (c, i, o)))
                        .filter(|(c, _, _)| super::context::load_checkpoint(ctx, c))
                        .map_or((0, 0, 0), |(c, i, o)| (c.n_tokens, i, o));
                    if keep > 0 {
                        tracing::info!(
                            "🧠 [Context] Restarted from the checkpoint at {} positions (sliding window)",
                            keep
                        );
                    }
                }
                if keep > 0 && !llama_cpp::llama_memory_seq_rm(memory, 0, keep, -1) {
                    (keep, first, skip) = (0, 0, 0);
                }
            }
            if keep == 0 {
                llama_cpp::llama_memory_clear(memory, true);
                (first, skip) = (0, 0);
            }
            llama.checkpoints.retain(|c| c.n_tokens <= keep);
            report.cache_n = keep as usize;
            let mut n_past = keep;
            let mut processed = layout[..first]
                .iter()
                .map(|chunk| match chunk {
                    CachedChunk::Text(tokens) => tokens.len(),
                    CachedChunk::Media { n_pos, .. } => *n_pos as usize,
                })
                .sum::<usize>() as u64
                + skip as u64;
            for (i, index) in chunk_targets.iter().enumerate().take(first) {
                if let (Some(p), Some(&(m, n))) =
                    (&self.progress, index.and_then(|x| self.targets.get(x)))
                {
                    if i == 0 || chunk_targets[i - 1] != *index {
                        p.emit(
                            Stage::Encoding,
                            State::Reused,
                            Some(p.attachment(m, n)),
                            None,
                        );
                    }
                }
            }
            if keep > 0 {
                tracing::info!(
                    "🧠 [Context] Media prompt reuses {} cached positions ({} of {} chunks), processing the rest",
                    keep, first, chunk_count
                );
            }
            for i in first..chunk_count {
                if llama.interrupt_signal.load(Ordering::Acquire)
                    || self.progress.as_ref().is_some_and(|p| p.cancelled())
                {
                    bail!("Media prefill cancelled");
                }
                let chunk = mtmd::mtmd_input_chunks_get(chunks.0.as_ptr(), i);
                let kind = mtmd::mtmd_input_chunk_get_type(chunk);
                let target = self.progress.as_ref().and_then(|p| {
                    chunk_targets[i]
                        .and_then(|index| self.targets.get(index))
                        .map(|&(m, n)| p.attachment(m, n))
                });
                let is_text = kind == mtmd::mtmd_input_chunk_type_MTMD_INPUT_CHUNK_TYPE_TEXT;
                let last = i + 1 == chunk_count;
                let code = if is_text && ((i == first && skip > 0) || (swa && last)) {
                    // Text processed here: the part not in the cache, and for a sliding-window model
                    // a checkpoint a few tokens before the prompt end (llama-server's offsets)
                    let CachedChunk::Text(tokens) = &layout[i] else {
                        bail!("Media layout does not match its chunks");
                    };
                    let rest = &tokens[if i == first { skip } else { 0 }..];
                    let split = if swa && last && rest.len() > super::context::CHECKPOINT_TAIL {
                        rest.len() - super::context::CHECKPOINT_TAIL
                    } else {
                        0
                    };
                    let mut code = decode_text(llama, &rest[..split], &mut n_past, false);
                    if code == 0 && split > 0 {
                        if let Some(checkpoint) =
                            super::context::take_checkpoint(llama.ctx_ptr, n_past)
                        {
                            super::context::remember_checkpoint(&mut llama.checkpoints, checkpoint);
                        }
                    }
                    if code == 0 {
                        code = decode_text(llama, &rest[split..], &mut n_past, last);
                    }
                    code
                } else if kind == mtmd::mtmd_input_chunk_type_MTMD_INPUT_CHUNK_TYPE_TEXT {
                    mtmd::mtmd_helper_eval_chunk_single(
                        projector.ptr.as_ptr(),
                        llama.ctx_ptr.cast(),
                        chunk,
                        n_past,
                        0,
                        i32::try_from(llama.n_batch)?,
                        i + 1 == chunk_count,
                        &mut n_past,
                    )
                } else {
                    if let Some(p) = &self.progress {
                        p.emit(Stage::Encoding, State::Running, target, None);
                    }
                    let encoded = mtmd::mtmd_encode_chunk(projector.ptr.as_ptr(), chunk);
                    if encoded != 0 {
                        bail!("Native media encoding failed with code {encoded}");
                    }
                    let index =
                        chunk_targets[i].context("Native media chunk lost attachment identity")?;
                    slice_done[index] += 1;
                    if let Some(p) = &self.progress {
                        p.emit(
                            Stage::Encoding,
                            if slice_done[index] == slice_totals[index] {
                                State::Completed
                            } else {
                                State::Running
                            },
                            target,
                            Some(ProcessingWork {
                                completed: slice_done[index],
                                total: slice_totals[index],
                                unit: "slices",
                            }),
                        );
                        p.emit(Stage::Prefill, State::Running, target, None);
                    }
                    let mut observation = DecodeObservation {
                        progress: self.progress.as_ref(),
                        target,
                        processed,
                        total: count as u64,
                    };
                    mtmd::mtmd_helper_decode_image_chunk(
                        projector.ptr.as_ptr(),
                        llama.ctx_ptr.cast(),
                        chunk,
                        mtmd::mtmd_get_output_embd(projector.ptr.as_ptr()),
                        n_past,
                        0,
                        i32::try_from(llama.n_batch)?,
                        &mut n_past,
                        Some(decoded_batch),
                        (&mut observation as *mut DecodeObservation<'_>).cast(),
                    )
                };
                if code != 0 || llama.interrupt_signal.load(Ordering::Acquire) {
                    llama_cpp::llama_memory_clear(memory, true);
                    llama.checkpoints.clear();
                    bail!("Native media evaluation failed/cancelled with code {code}");
                }
                processed += mtmd::mtmd_input_chunk_get_n_tokens(chunk) as u64;
                if let Some(p) = &self.progress {
                    p.emit(
                        Stage::Prefill,
                        State::Running,
                        target,
                        Some(ProcessingWork {
                            completed: processed,
                            total: count as u64,
                            unit: "tokens",
                        }),
                    );
                }
            }
            if n_past != positions {
                bail!("Native media evaluation position mismatch");
            }
            tracing::debug!(
                prompt_tokens = count,
                prompt_positions = positions,
                "Native media prefill evaluated"
            );
            if let Some(p) = &self.progress {
                p.emit(
                    Stage::Prefill,
                    State::Completed,
                    None,
                    Some(ProcessingWork {
                        completed: count as u64,
                        total: count as u64,
                        unit: "tokens",
                    }),
                );
                p.emit(Stage::Generation, State::Running, None, None);
            }
            // Native chunks own their preprocessed data; raw bitmap copies are no longer needed.
            self.bitmaps.clear();
            self.prompt_text_tokens = text_tokens.len();
            self.layout = layout;
            Ok(MediaPrefill {
                text_tokens,
                positions: n_past,
                context: report,
            })
        }
    }
}

/// Chunk index and offset of a KV position in `layout`: inside a text chunk, or at the start of a media
/// chunk (None inside media, which cannot be split).
fn text_position(layout: &[CachedChunk], pos: i32) -> Option<(usize, usize)> {
    let mut start = 0_i32;
    for (i, chunk) in layout.iter().enumerate() {
        match chunk {
            CachedChunk::Text(tokens) => {
                if pos < start + tokens.len() as i32 {
                    return Some((i, (pos - start) as usize));
                }
                start += tokens.len() as i32;
            }
            CachedChunk::Media { n_pos, .. } => {
                if pos == start {
                    return Some((i, 0));
                }
                if pos < start + n_pos {
                    return None;
                }
                start += n_pos;
            }
        }
    }
    None
}

/// Decodes text tokens at positions from `n_past` on; logits only for the last one when asked.
///
/// # Safety
/// `llama` is the live context the media prompt is being evaluated into.
unsafe fn decode_text(llama: &NativeLlama, tokens: &[i32], n_past: &mut i32, logits_last: bool) -> i32 {
    let step = llama.n_batch.max(1) as usize;
    let mut batch = super::context::SafeBatch {
        batch: llama_cpp::llama_batch_init(step as i32, 0, 1),
    };
    for (part_index, part) in tokens.chunks(step).enumerate() {
        let last_part = (part_index + 1) * step >= tokens.len();
        for (j, token) in part.iter().enumerate() {
            *batch.batch.token.add(j) = *token;
            *batch.batch.pos.add(j) = *n_past + j as i32;
            *batch.batch.n_seq_id.add(j) = 1;
            *(*batch.batch.seq_id.add(j)).add(0) = 0;
            *batch.batch.logits.add(j) = i8::from(logits_last && last_part && j + 1 == part.len());
        }
        batch.batch.n_tokens = part.len() as i32;
        let code = llama_cpp::llama_decode(llama.ctx_ptr, batch.batch);
        if code != 0 {
            return code;
        }
        *n_past += part.len() as i32;
    }
    0
}

struct DecodeObservation<'a> {
    progress: Option<&'a ProcessingReporter>,
    target: Option<AttachmentRef>,
    processed: u64,
    total: u64,
}

// SAFETY: upstream calls synchronously with a valid batch and our stack-owned observation.
unsafe extern "C" fn decoded_batch(
    batch: *const mtmd::mtmd_helper_embd_batch,
    user: *mut c_void,
) -> i32 {
    if batch.is_null() || user.is_null() {
        return 1;
    }
    let observation = unsafe { &mut *user.cast::<DecodeObservation<'_>>() };
    let tokens = unsafe { (*batch).n_tokens };
    if tokens < 0 {
        return 1;
    }
    observation.processed = observation.processed.saturating_add(tokens as u64);
    if let Some(p) = observation.progress {
        p.emit(
            Stage::Prefill,
            State::Running,
            observation.target,
            Some(ProcessingWork {
                completed: observation.processed,
                total: observation.total,
                unit: "tokens",
            }),
        );
        if p.cancelled() {
            return 1;
        }
    }
    0
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
