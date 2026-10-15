//! Lazy native video decoding into shared timestamped RGB buffers. No FFmpeg
//! dependency is linked into this crate or either inference backend.
pub mod abi;

use crate::native_assets::{load_library, symbol, NativeAssets};
use anyhow::{bail, Context, Result};
use cluaiz_core::{
    media::{DecodedAudio, DecodedImage, VideoSettings},
    media_limits::MediaLimits,
};
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const MAX_FORMAT_BYTES: usize = 64 * 1024;
const US_PER_MS: u64 = 1_000;
static FORMAT_CACHE: Mutex<Option<(String, Vec<String>)>> = Mutex::new(None);

pub struct TimestampedFrame {
    pub timestamp_us: u64,
    pub image: DecodedImage,
}
pub struct DecodedVideo {
    pub frames: Vec<TimestampedFrame>,
    pub audio: Option<DecodedAudio>,
    pub has_audio_track: bool,
    pub decoded_frames: u64,
}

struct Provider {
    api: abi::Api,
    _library: libloading::Library,
}
impl Provider {
    fn load(assets: &NativeAssets) -> Result<Self> {
        let video = assets
            .manifest
            .video
            .as_ref()
            .context("Packaged video decoder is unavailable")?;
        assets.verify()?;
        let library = load_library(&assets.library(&video.library)?)?;
        // SAFETY: versioned shared C ABI, verified package, and library retained
        // until every synchronous call and borrowed view have finished.
        let api = unsafe {
            let get = symbol::<abi::GetApi>(&library, abi::API_SYMBOL)?;
            let api = get().as_ref().context("Native video API is null")?;
            if api.version != abi::ABI_VERSION {
                bail!("Native video decoder ABI version mismatch");
            }
            *api
        };
        Ok(Self {
            api,
            _library: library,
        })
    }

    fn formats(&self) -> Result<Vec<String>> {
        // SAFETY: valid loaded API; first call is a size query, second has that capacity.
        let size = unsafe { (self.api.formats)(std::ptr::null_mut(), 0) };
        if size == 0 || size > MAX_FORMAT_BYTES {
            bail!("Native video format registry is invalid");
        }
        let mut bytes = vec![0; size];
        if unsafe { (self.api.formats)(bytes.as_mut_ptr(), size) } != size {
            bail!("Native video format registry changed");
        }
        let text = std::str::from_utf8(&bytes)?;
        Ok(text
            .lines()
            .filter(|extension| {
                !extension.is_empty() && extension.bytes().all(|c| c.is_ascii_alphanumeric())
            })
            .filter(|extension| {
                mime_guess::from_ext(extension)
                    .iter_raw()
                    .any(|mime| mime.starts_with("video/"))
            })
            .map(str::to_owned)
            .collect())
    }
}

/// Container descriptors from the actually loadable packaged decoder. A specific
/// file still needs a supported codec/valid track; this does not certify all video.
pub fn attachment_formats() -> Result<Vec<String>> {
    let assets = NativeAssets::from_environment()?;
    if let Ok(cache) = FORMAT_CACHE.lock() {
        if let Some((_, formats)) = cache
            .as_ref()
            .filter(|(identity, _)| identity == &assets.identity)
        {
            return Ok(formats.clone());
        }
    }
    let formats = Provider::load(&assets)?.formats()?;
    if let Ok(mut cache) = FORMAT_CACHE.lock() {
        *cache = Some((assets.identity, formats.clone()));
    }
    Ok(formats)
}

/// One synchronous bounded decode, suitable for a blocking worker. The caller's
/// cancellation flag is observed by demuxing, frame iteration and frame delivery.
pub fn decode(
    bytes: Vec<u8>,
    limits: MediaLimits,
    settings: VideoSettings,
    include_audio: bool,
    cancelled: Arc<AtomicBool>,
) -> Result<DecodedVideo> {
    decode_with_assets(
        bytes,
        limits,
        settings,
        include_audio,
        cancelled,
        &NativeAssets::from_environment()?,
    )
}

/// Explicit package injection for installers and isolated acceptance tests.
pub fn decode_with_assets(
    bytes: Vec<u8>,
    limits: MediaLimits,
    settings: VideoSettings,
    include_audio: bool,
    cancelled: Arc<AtomicBool>,
    assets: &NativeAssets,
) -> Result<DecodedVideo> {
    limits.check_input_bytes(u64::try_from(bytes.len())?)?;
    if bytes.is_empty() {
        bail!("Video source is empty");
    }
    let provider = Provider::load(assets)?;
    let mut consumer = Consumer {
        frames: Vec::new(),
        retained: 0,
        limits,
        cancelled,
        deadline: Instant::now()
            .checked_add(Duration::from_millis(settings.deadline_ms.get()))
            .context("Video deadline overflow")?,
        error: None,
    };
    let request = abi::Request {
        bytes: bytes.as_ptr(),
        byte_count: bytes.len(),
        interval_us: settings
            .sample_interval_ms
            .get()
            .checked_mul(US_PER_MS)
            .context("Video interval overflow")?,
        max_duration_us: settings
            .max_duration_ms
            .get()
            .checked_mul(US_PER_MS)
            .context("Video duration overflow")?,
        max_frames: settings.max_frames.get(),
        max_pixels: settings.max_frame_pixels.get(),
        max_packets: settings.max_packets.get(),
        max_decoded_frames: settings.max_decoded_frames.get(),
        max_workspace_bytes: limits.max_decoded_bytes(),
        context: (&mut consumer as *mut Consumer).cast(),
        cancelled: is_cancelled,
        frame: collect_frame,
    };
    let mut info = abi::Info::default();
    let mut error = [0_u8; abi::ERROR_BYTES];
    // SAFETY: exact shared ABI; input, callbacks and unique Consumer are live for
    // this call. All native owners are destroyed before the library is dropped.
    let status =
        unsafe { (provider.api.decode)(&request, &mut info, error.as_mut_ptr(), error.len()) };
    if let Some(error) = consumer.error.take() {
        return Err(error);
    }
    if status != 0 {
        let end = error
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(error.len());
        bail!("{}", String::from_utf8_lossy(&error[..end]));
    }
    consumer.check_cancel()?;
    if consumer.frames.is_empty() || consumer.frames.len() != info.sampled_frames as usize {
        bail!("Native video decoder returned inconsistent frame metadata");
    }
    let audio = if include_audio {
        if info.has_audio == 0 {
            bail!("Video audio was requested but no audio track exists");
        }
        let remaining = limits
            .max_decoded_bytes()
            .checked_sub(consumer.retained)
            .context("Video audio budget exhausted")?;
        let audio = crate::audio::decode_audio(
            bytes,
            MediaLimits::new(limits.max_input_bytes(), remaining)?,
        )?;
        consumer.check_cancel()?;
        Some(audio)
    } else {
        None
    };
    Ok(DecodedVideo {
        frames: consumer.frames,
        audio,
        has_audio_track: info.has_audio != 0,
        decoded_frames: info.decoded_frames,
    })
}

struct Consumer {
    frames: Vec<TimestampedFrame>,
    retained: u64,
    limits: MediaLimits,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    error: Option<anyhow::Error>,
}

impl Consumer {
    fn check_cancel(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.deadline {
            bail!("Video preparation cancelled or deadline exceeded");
        }
        Ok(())
    }

    fn collect(&mut self, frame: &abi::Frame) -> Result<()> {
        self.check_cancel()?;
        let row_bytes = usize::try_from(frame.width)?
            .checked_mul(cluaiz_core::media::RGB8_CHANNELS)
            .context("Video RGB row overflow")?;
        let rows = usize::try_from(frame.height)?;
        let size = row_bytes
            .checked_mul(rows)
            .context("Video RGB size overflow")?;
        let required = rows
            .checked_sub(1)
            .and_then(|rows| rows.checked_mul(frame.stride))
            .and_then(|v| v.checked_add(row_bytes))
            .context("Invalid video frame dimensions")?;
        if frame.pixels.is_null()
            || row_bytes == 0
            || frame.stride < row_bytes
            || required > frame.pixel_bytes
            || frame.pixel_bytes > isize::MAX as usize
        {
            bail!("Native video frame has invalid RGB row layout");
        }
        self.limits.check_decoded_bytes(
            self.retained
                .checked_add(
                    u64::try_from(size)?
                        .checked_mul(2)
                        .context("Video copy budget overflow")?,
                )
                .context("Video retained count overflow")?,
        )?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(size)?;
        // SAFETY: provider guarantees a live view for this callback; dimensions,
        // stride and final accessible byte have been checked before copying rows.
        let native = unsafe { std::slice::from_raw_parts(frame.pixels, frame.pixel_bytes) };
        for row in 0..rows {
            pixels.extend_from_slice(&native[row * frame.stride..row * frame.stride + row_bytes]);
        }
        let image = DecodedImage::new(frame.width, frame.height, Arc::from(pixels))?;
        self.frames.try_reserve(1)?;
        self.frames.push(TimestampedFrame {
            timestamp_us: frame.timestamp_us,
            image,
        });
        self.retained = self
            .retained
            .checked_add(u64::try_from(size)?)
            .context("Video retained count overflow")?;
        Ok(())
    }
}

// SAFETY: provider calls synchronously on the current thread with our live Consumer.
unsafe extern "C" fn is_cancelled(context: *mut c_void) -> i32 {
    i32::from((&*context.cast::<Consumer>()).check_cancel().is_err())
}

// SAFETY: this is the unique synchronous Consumer borrow; frame expires at return.
// Return errors through the C status instead of unwinding across the native ABI.
unsafe extern "C" fn collect_frame(context: *mut c_void, frame: *const abi::Frame) -> i32 {
    let consumer = &mut *context.cast::<Consumer>();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        consumer.collect(frame.as_ref().context("Native video frame is null")?)
    }));
    match result {
        Ok(Ok(())) => 0,
        other => {
            consumer.error = Some(match other {
                Ok(Err(error)) => error,
                _ => anyhow::anyhow!("Video frame consumer unwound"),
            });
            -1
        }
    }
}
