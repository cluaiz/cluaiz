use super::*;
use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
};

struct Collector {
    frames: Vec<(u64, [u8; 3])>,
    cancelled: AtomicBool,
    stop_after_first: bool,
}

// SAFETY: each request holds a uniquely borrowed Collector through decode.
unsafe extern "C" fn cancelled(context: *mut c_void) -> i32 {
    i32::from(
        (&*context.cast::<Collector>())
            .cancelled
            .load(Ordering::Acquire),
    )
}
// SAFETY: native frame buffer is live in this synchronous callback.
unsafe extern "C" fn collect(context: *mut c_void, frame: *const abi::Frame) -> i32 {
    let collector = &mut *context.cast::<Collector>();
    let frame = &*frame;
    if frame.width != 34 || frame.height != 26 || frame.pixel_bytes < 3 {
        return -1;
    }
    collector.frames.push((
        frame.timestamp_us,
        [*frame.pixels, *frame.pixels.add(1), *frame.pixels.add(2)],
    ));
    if collector.stop_after_first {
        collector.cancelled.store(true, Ordering::Release);
    }
    0
}

fn invoke(bytes: &[u8], stop_after_first: bool) -> (i32, Collector, abi::Info, String) {
    let mut collector = Collector {
        frames: Vec::new(),
        cancelled: AtomicBool::new(false),
        stop_after_first,
    };
    let request = abi::Request {
        bytes: bytes.as_ptr(),
        byte_count: bytes.len(),
        interval_us: 100_000,
        max_frames: 16,
        max_duration_us: 30_000_000,
        max_pixels: 4_194_304,
        max_workspace_bytes: 256 * 1024 * 1024,
        max_packets: 100_000,
        max_decoded_frames: 10_000,
        context: (&mut collector as *mut Collector).cast(),
        cancelled,
        frame: collect,
    };
    let mut info = abi::Info::default();
    let mut error = [0_u8; abi::ERROR_BYTES];
    // SAFETY: all borrowed input, output and callback state outlive decode.
    let status = unsafe { decode(&request, &mut info, error.as_mut_ptr(), error.len()) };
    let end = error.iter().position(|b| *b == 0).unwrap_or(error.len());
    (
        status,
        collector,
        info,
        String::from_utf8_lossy(&error[..end]).into(),
    )
}

#[test]
#[ignore = "requires actual encoded CLUAIZ_VIDEO_TEST_FIXTURES"]
fn real_native_abi_memory_input_timestamps_mid_decode_cancel_and_playlist_rejection() -> Result<()>
{
    let root = std::path::PathBuf::from(std::env::var("CLUAIZ_VIDEO_TEST_FIXTURES")?);
    for (file, timestamps, audio) in [
        (
            "red-blue-audio.mp4",
            vec![0, 500_000, 1_000_000, 1_500_000],
            1,
        ),
        (
            "red-blue-silent.mp4",
            vec![0, 500_000, 1_000_000, 1_500_000],
            0,
        ),
        (
            "red-blue-vfr.mp4",
            vec![0, 300_000, 1_100_000, 1_800_000],
            0,
        ),
    ] {
        let bytes = std::fs::read(root.join(file))?;
        let (status, collector, info, error) = invoke(&bytes, false);
        assert_eq!(status, 0, "{error}");
        assert_eq!(info.has_audio, audio);
        assert_eq!(info.sampled_frames, 4);
        assert_eq!(
            collector.frames.iter().map(|f| f.0).collect::<Vec<_>>(),
            timestamps
        );
        for (index, (_, rgb)) in collector.frames.iter().enumerate() {
            if index < 2 {
                assert!(rgb[0] > 220 && rgb[2] < 45);
            } else {
                assert!(rgb[2] > 220 && rgb[0] < 45);
            }
        }
        let (status, collector, _, error) = invoke(&bytes, true);
        assert_ne!(status, 0, "{error}");
        assert_eq!(collector.frames.len(), 1);
        assert!(error.contains("cancelled"), "{error}");
    }
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let playlist = format!("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\nhttp://{}/segment.ts\n#EXT-X-ENDLIST\n", listener.local_addr()?);
    let (status, _, _, _) = invoke(playlist.as_bytes(), false);
    assert_ne!(status, 0);
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    println!("REAL VIDEO ABI: {}-{}; borrowed-memory CFR/VFR RGB; audio-track metadata; cancellation after first emitted frame; playlist rejected without a network connection", std::env::consts::OS, std::env::consts::ARCH);
    Ok(())
}
