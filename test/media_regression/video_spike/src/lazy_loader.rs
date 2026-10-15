//! Real package load with no compile-time FFmpeg dependency in this test binary.
#[path = "../../../../engine/media/src/video/abi.rs"]
mod abi;
use anyhow::{Context, Result};
use std::{ffi::c_void, path::PathBuf};

struct Frames(Vec<(u64, [u8; 3])>);
unsafe extern "C" fn cancelled(_: *mut c_void) -> i32 {
    0
}
// SAFETY: provider calls synchronously with our live Frames and native RGB view.
unsafe extern "C" fn frame(context: *mut c_void, frame: *const abi::Frame) -> i32 {
    let output = &mut *context.cast::<Frames>();
    let frame = &*frame;
    if frame.width != 34 || frame.height != 26 || frame.pixels.is_null() || frame.pixel_bytes < 3 {
        return -1;
    }
    output.0.push((
        frame.timestamp_us,
        [*frame.pixels, *frame.pixels.add(1), *frame.pixels.add(2)],
    ));
    0
}

#[test]
#[ignore = "requires staged CLUAIZ_VIDEO_TEST_PACKAGE and genuine CLUAIZ_VIDEO_TEST_FIXTURES"]
fn real_package_loads_without_sdk_search_path_and_decodes_via_shared_abi() -> Result<()> {
    let root = PathBuf::from(std::env::var("CLUAIZ_VIDEO_TEST_PACKAGE")?);
    let path = root.join(format!(
        "{}cluaiz_video_ffmpeg{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let bytes = std::fs::read(
        PathBuf::from(std::env::var("CLUAIZ_VIDEO_TEST_FIXTURES")?).join("red-blue-audio.mp4"),
    )?;
    // SAFETY: selected actual test package only. Native views/handles finish
    // before this scoped library owner is destroyed. No SDK/PATH setup is used.
    unsafe {
        #[cfg(windows)]
        let library: libloading::Library = libloading::os::windows::Library::load_with_flags(
            &path,
            libloading::os::windows::LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR
                | libloading::os::windows::LOAD_LIBRARY_SEARCH_SYSTEM32,
        )?
        .into();
        #[cfg(not(windows))]
        let library = libloading::Library::new(&path)?;
        let get = library.get::<abi::GetApi>(abi::API_SYMBOL)?;
        let api = get().as_ref().context("Null native API")?;
        assert_eq!(api.version, abi::ABI_VERSION);
        let mut output = Frames(Vec::new());
        let request = abi::Request {
            bytes: bytes.as_ptr(),
            byte_count: bytes.len(),
            interval_us: 500_000,
            // H264 may allocate padded coded dimensions beyond visible 34x26.
            max_frames: 4,
            max_duration_us: 3_000_000,
            max_pixels: 4096,
            max_workspace_bytes: 1024 * 1024,
            max_packets: 1000,
            max_decoded_frames: 10,
            context: (&mut output as *mut Frames).cast(),
            cancelled,
            frame,
        };
        let mut info = abi::Info::default();
        let mut error = [0; abi::ERROR_BYTES];
        let status = (api.decode)(&request, &mut info, error.as_mut_ptr(), error.len());
        let end = error
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(error.len());
        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&error[..end]));
        assert_eq!(info.sampled_frames, 4);
        assert_eq!(info.has_audio, 1);
        assert_eq!(
            output.0.iter().map(|f| f.0).collect::<Vec<_>>(),
            [0, 500_000, 1_000_000, 1_500_000]
        );
        assert!(output.0[0].1[0] > 220 && output.0[3].1[2] > 220);
        println!("REAL LAZY VIDEO PACKAGE: {}-{}; no linked FFmpeg SDK; staged provider/dependencies -> shared ABI -> actual RGB/timestamps passed", std::env::consts::OS, std::env::consts::ARCH);
    }
    Ok(())
}
