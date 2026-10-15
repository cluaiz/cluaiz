//! Versioned synchronous C boundary. This exact source is also compiled by the
//! isolated provider; no Rust Vec/String/Result or allocator ownership crosses it.

use std::ffi::c_void;

pub const ABI_VERSION: u32 = 1;
pub const ERROR_BYTES: usize = 1024;
pub const API_SYMBOL: &[u8] = b"cluaiz_video_api_v1\0";

#[repr(C)]
pub struct Frame {
    pub timestamp_us: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: *const u8,
    pub pixel_bytes: usize,
    pub stride: usize,
}

/// All borrowed pointers remain valid until decode returns. Callbacks are invoked
/// synchronously on that calling thread; a frame view expires at callback return.
#[repr(C)]
pub struct Request {
    pub bytes: *const u8,
    pub byte_count: usize,
    pub interval_us: u64,
    pub max_frames: u32,
    pub max_duration_us: u64,
    pub max_pixels: u64,
    pub max_workspace_bytes: u64,
    pub max_packets: u64,
    pub max_decoded_frames: u64,
    pub context: *mut c_void,
    pub cancelled: unsafe extern "C" fn(*mut c_void) -> i32,
    pub frame: unsafe extern "C" fn(*mut c_void, *const Frame) -> i32,
}

#[derive(Default)]
#[repr(C)]
pub struct Info {
    pub has_audio: i32,
    pub decoded_frames: u64,
    pub sampled_frames: u32,
}

pub type Decode = unsafe extern "C" fn(*const Request, *mut Info, *mut u8, usize) -> i32;
pub type Formats = unsafe extern "C" fn(*mut u8, usize) -> usize;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Api {
    pub version: u32,
    pub decode: Decode,
    /// Newline-separated container extensions from the linked demuxer registry.
    /// Returns required bytes. No writes when the supplied buffer is too short.
    pub formats: Formats,
}

pub type GetApi = unsafe extern "C" fn() -> *const Api;
