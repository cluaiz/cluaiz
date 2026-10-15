//! Actual native video decoder. Built separately and loaded only for video use.
#[path = "../../../src/video/abi.rs"]
pub mod abi;
mod memory_input;

use anyhow::{bail, Context, Result};
use ffmpeg_next as ffmpeg;
use std::{collections::BTreeSet, ffi::CStr, ptr};

const MICROSECONDS: u64 = 1_000_000;
// Conservative guard for retained pixels plus current native/conversion buffers;
// it is not an OS-wide allocator ceiling or a measurement of codec internals.
const FRAME_WORKSPACE_BYTES_PER_PIXEL: u64 = 32;

static API: abi::Api = abi::Api {
    version: abi::ABI_VERSION,
    decode,
    formats,
};

#[no_mangle]
pub extern "C" fn cluaiz_video_api_v1() -> *const abi::Api {
    &API
}

// SAFETY: caller owns valid request/output buffers for this synchronous call.
// Native/Rust allocations are always destroyed on this side of the ABI.
unsafe extern "C" fn decode(
    request: *const abi::Request,
    info: *mut abi::Info,
    error: *mut u8,
    error_len: usize,
) -> i32 {
    if request.is_null() || info.is_null() {
        return -1;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&*request)));
    match result {
        Ok(Ok(result)) => {
            *info = result;
            0
        }
        other => {
            let message = match other {
                Ok(Err(e)) => format!("{e:#}"),
                _ => "Native video decoder unwound".into(),
            };
            if !error.is_null() && error_len > 0 {
                let count = message.len().min(error_len - 1);
                ptr::copy_nonoverlapping(message.as_ptr(), error, count);
                *error.add(count) = 0;
            }
            -1
        }
    }
}

fn check_cancel(request: &abi::Request) -> Result<()> {
    // SAFETY: request contract retains callback context through this call.
    if unsafe { (request.cancelled)(request.context) } != 0 {
        bail!("Video preparation cancelled or deadline exceeded");
    }
    Ok(())
}

fn run(request: &abi::Request) -> Result<abi::Info> {
    if request.bytes.is_null()
        || request.byte_count == 0
        || request.byte_count > isize::MAX as usize
        || request.interval_us == 0
        || request.max_frames == 0
        || request.max_duration_us == 0
        || request.max_pixels == 0
        || request.max_packets == 0
        || request.max_decoded_frames == 0
    {
        bail!("Invalid native video request");
    }
    check_cancel(request)?;
    ffmpeg::init()?;
    // SAFETY: input owner remains alive through this synchronous call.
    let bytes = unsafe { std::slice::from_raw_parts(request.bytes, request.byte_count) };
    let mut source = memory_input::MemoryInput::open(bytes, request)?;
    check_cancel(request)?;
    if source.input.duration() > 0 && source.input.duration() as u64 > request.max_duration_us {
        bail!("Video exceeds configured duration limit");
    }
    let has_audio = source
        .input
        .streams()
        .any(|stream| stream.parameters().medium() == ffmpeg::media::Type::Audio);
    let stream = source
        .input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .context("No video track found")?;
    let index = stream.index();
    let time_base = stream.time_base();
    if time_base.numerator() <= 0 || time_base.denominator() <= 0 {
        bail!("Video has invalid timestamp time base");
    }
    let mut context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
    // SAFETY: exclusive, valid codec context, before opening the decoder. FFmpeg
    // checks max_pixels while parsing frames; one decoder thread limits concurrency.
    unsafe {
        (*context.as_mut_ptr()).max_pixels = i64::try_from(request.max_pixels)?;
        (*context.as_mut_ptr()).thread_count = 1;
    }
    let mut decoder = context.decoder().video()?;
    let mut sampling = Sampling {
        request,
        time_base,
        origin: None,
        last: None,
        next: 0,
        retained_bytes: 0,
        info: abi::Info {
            has_audio: i32::from(has_audio),
            ..Default::default()
        },
    };
    let mut packets = 0_u64;
    loop {
        check_cancel(request)?;
        let mut packet = ffmpeg::Packet::empty();
        match packet.read(&mut source.input) {
            Ok(()) => {
                packets = packets
                    .checked_add(1)
                    .context("Video packet count overflow")?;
                if packets > request.max_packets {
                    bail!("Video exceeds configured packet limit");
                }
                if packet.stream() == index {
                    decoder.send_packet(&packet)?;
                    sampling.receive(&mut decoder)?;
                }
            }
            Err(ffmpeg::Error::Eof) => break,
            Err(error) => return Err(error).context("Cannot read video packet"),
        }
    }
    decoder.send_eof()?;
    sampling.receive(&mut decoder)?;
    if sampling.info.sampled_frames == 0 {
        bail!("Video contained no decoded frames");
    }
    Ok(sampling.info)
}

struct Sampling<'a> {
    request: &'a abi::Request,
    time_base: ffmpeg::Rational,
    origin: Option<i64>,
    last: Option<u64>,
    next: u64,
    retained_bytes: u64,
    info: abi::Info,
}

impl Sampling<'_> {
    fn receive(&mut self, decoder: &mut ffmpeg::decoder::Video) -> Result<()> {
        loop {
            check_cancel(self.request)?;
            let mut frame = ffmpeg::frame::Video::empty();
            match decoder.receive_frame(&mut frame) {
                Ok(()) => {}
                Err(ffmpeg::Error::Eof) => return Ok(()),
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {
                    return Ok(())
                }
                Err(error) => return Err(error).context("Cannot decode video frame"),
            }
            self.info.decoded_frames = self
                .info
                .decoded_frames
                .checked_add(1)
                .context("Video frame count overflow")?;
            if self.info.decoded_frames > self.request.max_decoded_frames {
                bail!("Video exceeds decoded frame limit");
            }
            let pts = frame
                .timestamp()
                .context("Video frame has no presentation timestamp")?;
            let origin = *self.origin.get_or_insert(pts);
            let delta = i128::from(pts) - i128::from(origin);
            let timestamp = delta
                .checked_mul(i128::from(self.time_base.numerator()))
                .and_then(|v| v.checked_mul(i128::from(MICROSECONDS)))
                .context("Video timestamp overflow")?
                / i128::from(self.time_base.denominator());
            let timestamp_us = u64::try_from(timestamp)
                .context("Video timestamps moved before the first frame")?;
            if self.last.is_some_and(|last| timestamp_us < last) {
                bail!("Video presentation timestamps are out of order");
            }
            self.last = Some(timestamp_us);
            if timestamp_us > self.request.max_duration_us {
                bail!("Video exceeds configured duration limit");
            }
            if timestamp_us < self.next {
                continue;
            }
            if self.info.sampled_frames >= self.request.max_frames {
                bail!("Video exceeds sampled frame limit; increase sampling interval or shorten video");
            }
            self.emit(&frame, timestamp_us)?;
            self.next = (timestamp_us / self.request.interval_us)
                .checked_add(1)
                .and_then(|v| v.checked_mul(self.request.interval_us))
                .context("Video sampling timestamp overflow")?;
        }
    }

    fn emit(&mut self, frame: &ffmpeg::frame::Video, timestamp_us: u64) -> Result<()> {
        let pixels = u64::from(frame.width())
            .checked_mul(u64::from(frame.height()))
            .context("Video pixel count overflow")?;
        if pixels == 0 || pixels > self.request.max_pixels {
            bail!("Video frame exceeds pixel limit");
        }
        let workspace = pixels
            .checked_mul(FRAME_WORKSPACE_BYTES_PER_PIXEL)
            .and_then(|size| size.checked_add(self.retained_bytes))
            .context("Video workspace count overflow")?;
        if workspace > self.request.max_workspace_bytes {
            bail!("Video exceeds decoded workspace budget");
        }
        let mut scaler = ffmpeg::software::scaling::context::Context::get(
            frame.format(),
            frame.width(),
            frame.height(),
            ffmpeg::format::Pixel::RGB24,
            frame.width(),
            frame.height(),
            ffmpeg::software::scaling::flag::Flags::BILINEAR,
        )?;
        let mut rgb = ffmpeg::frame::Video::empty();
        scaler.run(frame, &mut rgb)?;
        check_cancel(self.request)?;
        let data = rgb.data(0);
        let view = abi::Frame {
            timestamp_us,
            width: rgb.width(),
            height: rgb.height(),
            pixels: data.as_ptr(),
            pixel_bytes: data.len(),
            stride: rgb.stride(0),
        };
        // SAFETY: borrowed pixel view and callback context outlive the synchronous call.
        if unsafe { (self.request.frame)(self.request.context, &view) } != 0 {
            bail!("Video frame consumer stopped preparation");
        }
        self.retained_bytes = self
            .retained_bytes
            .checked_add(pixels.checked_mul(3).context("RGB byte count overflow")?)
            .context("Video retained count overflow")?;
        self.info.sampled_frames += 1;
        Ok(())
    }
}

// SAFETY: output is caller-owned; size query never writes, and the exact returned
// length is checked before copying. FFmpeg owns all iterated static descriptors.
unsafe extern "C" fn formats(output: *mut u8, capacity: usize) -> usize {
    let mut opaque = ptr::null_mut();
    let mut extensions = BTreeSet::new();
    loop {
        let format = ffmpeg::ffi::av_demuxer_iterate(&mut opaque);
        if format.is_null() {
            break;
        }
        if !(*format).extensions.is_null() {
            if let Ok(text) = CStr::from_ptr((*format).extensions).to_str() {
                for extension in text
                    .split(',')
                    .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric()))
                {
                    extensions.insert(extension.to_string());
                }
            }
        }
    }
    let text = extensions.into_iter().collect::<Vec<_>>().join("\n");
    if !output.is_null() && capacity >= text.len() {
        ptr::copy_nonoverlapping(text.as_ptr(), output, text.len());
    }
    text.len()
}

#[cfg(test)]
mod tests;
