//! Custom read/seek input, with all secondary protocol/file opens denied.
use crate::{abi, ffmpeg};
use anyhow::{bail, Context, Result};
use ffmpeg::ffi;
use std::{
    ffi::{c_int, c_void},
    ptr,
};

const IO_BUFFER_BYTES: usize = 32 * 1024;
const MAX_PROBE_BYTES: i64 = 4 * 1024 * 1024;
const MAX_ANALYZE_US: i64 = 5_000_000;
const MAX_STREAMS: i32 = 16;

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    request: &'a abi::Request,
}

// SAFETY: FFmpeg calls synchronously with our boxed Reader alive and its own
// writable output buffer of the given length. Only that input allocation is read.
unsafe extern "C" fn read(opaque: *mut c_void, output: *mut u8, count: c_int) -> c_int {
    let reader = &mut *opaque.cast::<Reader<'_>>();
    if (reader.request.cancelled)(reader.request.context) != 0 {
        return ffi::AVERROR_EXIT;
    }
    if count <= 0 {
        return ffi::AVERROR(ffmpeg::error::EINVAL);
    }
    let size = (count as usize).min(reader.bytes.len().saturating_sub(reader.position));
    if size == 0 {
        return ffi::AVERROR_EOF;
    }
    ptr::copy_nonoverlapping(reader.bytes.as_ptr().add(reader.position), output, size);
    reader.position += size;
    size as c_int
}

// SAFETY: the same scoped Reader is retained through format-context destruction.
unsafe extern "C" fn seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    let reader = &mut *opaque.cast::<Reader<'_>>();
    if (reader.request.cancelled)(reader.request.context) != 0 {
        return i64::from(ffi::AVERROR_EXIT);
    }
    if whence == ffi::AVSEEK_SIZE {
        return reader.bytes.len() as i64;
    }
    let base = match whence & !ffi::AVSEEK_FORCE {
        0 => 0_i128,
        1 => reader.position as i128,
        2 => reader.bytes.len() as i128,
        _ => return i64::from(ffi::AVERROR(ffmpeg::error::EINVAL)),
    };
    let next = base + i128::from(offset);
    if next < 0 || next > reader.bytes.len() as i128 {
        return i64::from(ffi::AVERROR(ffmpeg::error::EINVAL));
    }
    reader.position = next as usize;
    reader.position as i64
}

// No demuxer may open an attachment-selected nested URL, playlist or local file.
unsafe extern "C" fn deny_open(
    _: *mut ffi::AVFormatContext,
    _: *mut *mut ffi::AVIOContext,
    _: *const std::ffi::c_char,
    _: c_int,
    _: *mut *mut ffi::AVDictionary,
) -> c_int {
    ffi::AVERROR(ffmpeg::error::EPERM)
}

// SAFETY: opaque is the live boxed Reader used by read/seek.
unsafe extern "C" fn interrupt(opaque: *mut c_void) -> c_int {
    let reader = &*opaque.cast::<Reader<'_>>();
    (reader.request.cancelled)(reader.request.context)
}

struct Io(*mut ffi::AVIOContext);
impl Drop for Io {
    fn drop(&mut self) {
        // SAFETY: avio_alloc_context transfers this IO/buffer to this unique owner.
        // FFmpeg can replace the buffer, so free the current one, then the context.
        unsafe {
            ffi::av_free((*self.0).buffer.cast());
            (*self.0).buffer = ptr::null_mut();
            ffi::avio_context_free(&mut self.0);
        }
    }
}

pub struct MemoryInput<'a> {
    // Declaration order keeps IO and callback data alive until input closes.
    pub input: ffmpeg::format::context::Input,
    _io: Io,
    _reader: Box<Reader<'a>>,
}

impl<'a> MemoryInput<'a> {
    pub fn open(bytes: &'a [u8], request: &'a abi::Request) -> Result<Self> {
        let mut reader = Box::new(Reader {
            bytes,
            position: 0,
            request,
        });
        let opaque = (&mut *reader as *mut Reader<'a>).cast();
        // SAFETY: checked allocations, synchronous borrowed Reader lifetime and
        // explicit unique ownership follow FFmpeg's custom AVIO contract.
        unsafe {
            let buffer = ffi::av_malloc(IO_BUFFER_BYTES).cast::<u8>();
            if buffer.is_null() {
                bail!("Cannot allocate video IO buffer");
            }
            let io = ffi::avio_alloc_context(
                buffer,
                IO_BUFFER_BYTES as i32,
                0,
                opaque,
                Some(read),
                None,
                Some(seek),
            );
            if io.is_null() {
                ffi::av_free(buffer.cast());
                bail!("Cannot allocate video IO context");
            }
            let io = Io(io);
            let mut format = ffi::avformat_alloc_context();
            if format.is_null() {
                bail!("Cannot allocate video format context");
            }
            (*format).pb = io.0;
            (*format).flags |= ffi::AVFMT_FLAG_CUSTOM_IO;
            (*format).probesize = MAX_PROBE_BYTES.min(bytes.len() as i64);
            (*format).max_analyze_duration = MAX_ANALYZE_US;
            (*format).max_streams = MAX_STREAMS;
            (*format).io_open = Some(deny_open);
            (*format).interrupt_callback = ffi::AVIOInterruptCB {
                callback: Some(interrupt),
                opaque,
            };
            let status =
                ffi::avformat_open_input(&mut format, ptr::null(), ptr::null(), ptr::null_mut());
            if status < 0 {
                // open_input frees/nulls a failed context, but still close any retained one.
                ffi::avformat_close_input(&mut format);
                return Err(ffmpeg::Error::from(status)).context("Cannot probe video bytes");
            }
            let mut input = ffmpeg::format::context::Input::wrap(format);
            let status = ffi::avformat_find_stream_info(input.as_mut_ptr(), ptr::null_mut());
            if status < 0 {
                return Err(ffmpeg::Error::from(status)).context("Cannot inspect video tracks");
            }
            Ok(Self {
                input,
                _io: io,
                _reader: reader,
            })
        }
    }
}
