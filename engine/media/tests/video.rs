//! Real packaged decoder acceptance. Fixtures are genuine encoded videos supplied
//! by the test runner; production binaries contain no fixture encoder.
use anyhow::{Context, Result};
use cluaiz_core::{
    media::{InputPart, PreparedPart, VideoSettings},
    media_limits::MediaLimits,
    ChatEnvelope, ChatMessage,
};
use cluaiz_media::{native_assets::NativeAssets, video};
use std::{
    num::{NonZeroU32, NonZeroU64},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

fn settings() -> VideoSettings {
    VideoSettings {
        sample_interval_ms: NonZeroU64::new(500).unwrap(),
        ..Default::default()
    }
}

fn inspect(video: &video::DecodedVideo, timestamps: &[u64]) {
    assert_eq!(video.frames.len(), timestamps.len());
    for (index, (frame, timestamp)) in video.frames.iter().zip(timestamps).enumerate() {
        assert_eq!(frame.timestamp_us, *timestamp);
        assert_eq!((frame.image.width(), frame.image.height()), (34, 26));
        assert_eq!(frame.image.pixels().len(), 34 * 26 * 3);
        for pixel in frame.image.pixels().chunks_exact(3) {
            if index < 2 {
                assert!(
                    pixel[0] > 220 && pixel[1] < 45 && pixel[2] < 45,
                    "{pixel:?}"
                );
            } else {
                assert!(
                    pixel[2] > 220 && pixel[0] < 45 && pixel[1] < 45,
                    "{pixel:?}"
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires actual CLUAIZ_VIDEO_TEST_PACKAGE / CLUAIZ_VIDEO_TEST_FIXTURES and isolated CLUAIZ_HOME"]
async fn real_packaged_video_pixels_timestamps_audio_limits_and_shared_request() -> Result<()> {
    let package = PathBuf::from(std::env::var("CLUAIZ_VIDEO_TEST_PACKAGE")?);
    let fixtures = PathBuf::from(std::env::var("CLUAIZ_VIDEO_TEST_FIXTURES")?);
    let assets = NativeAssets::read(&package)?;
    let bytes = std::fs::read(fixtures.join("red-blue-audio.mp4"))?;
    let limits = MediaLimits::default();
    let decode = |bytes, options, audio, flag| {
        video::decode_with_assets(bytes, limits, options, audio, flag, &assets)
    };
    let flag = || Arc::new(AtomicBool::new(false));
    let actual = decode(bytes.clone(), settings(), true, flag())?;
    inspect(&actual, &[0, 500_000, 1_000_000, 1_500_000]);
    assert!(actual.has_audio_track);
    assert_eq!(actual.decoded_frames, 4);
    let audio = actual
        .audio
        .as_ref()
        .context("Actual audio track was lost")?;
    assert_eq!((audio.sample_rate(), audio.channels()), (16_000, 1));
    assert!(audio.samples().len() >= 32_000);
    assert!(audio.samples().iter().any(|sample| sample.abs() > 0.05));
    let no_audio_bytes = std::fs::read(fixtures.join("red-blue-silent.mp4"))?;
    let silent = decode(no_audio_bytes.clone(), settings(), false, flag())?;
    inspect(&silent, &[0, 500_000, 1_000_000, 1_500_000]);
    assert!(!silent.has_audio_track && silent.audio.is_none());
    assert!(decode(no_audio_bytes, settings(), true, flag()).is_err());
    let mut vfr_settings = settings();
    vfr_settings.sample_interval_ms = NonZeroU64::new(100).unwrap();
    let vfr = decode(
        std::fs::read(fixtures.join("red-blue-vfr.mp4"))?,
        vfr_settings,
        false,
        flag(),
    )?;
    inspect(&vfr, &[0, 300_000, 1_100_000, 1_800_000]);
    assert!(decode(b"corrupt video bytes".to_vec(), settings(), false, flag()).is_err());
    assert!(decode(Vec::new(), settings(), false, flag()).is_err());
    let mut options = settings();
    options.max_frames = NonZeroU32::new(1).unwrap();
    assert!(decode(bytes.clone(), options, false, flag()).is_err());
    options = settings();
    options.max_duration_ms = NonZeroU64::new(100).unwrap();
    assert!(decode(bytes.clone(), options, false, flag()).is_err());
    options = settings();
    options.max_frame_pixels = NonZeroU64::new(10).unwrap();
    assert!(decode(bytes.clone(), options, false, flag()).is_err());
    options = settings();
    options.max_packets = NonZeroU64::new(1).unwrap();
    assert!(decode(bytes.clone(), options, false, flag()).is_err());
    options = settings();
    options.max_decoded_frames = NonZeroU64::new(1).unwrap();
    assert!(decode(bytes.clone(), options, false, flag()).is_err());
    let cancelled = flag();
    cancelled.store(true, Ordering::Release);
    assert!(decode(bytes.clone(), settings(), false, cancelled).is_err());
    assert!(video::decode_with_assets(
        bytes.clone(),
        MediaLimits::new(1024 * 1024, 100)?,
        settings(),
        false,
        flag(),
        &assets
    )
    .is_err());
    let (first, second) = std::thread::scope(|scope| -> Result<_> {
        let first = scope.spawn(|| decode(bytes.clone(), settings(), false, flag()));
        let second = decode(bytes.clone(), settings(), false, flag())?;
        Ok((
            first
                .join()
                .map_err(|_| anyhow::anyhow!("Decoder thread unwound"))??,
            second,
        ))
    })?;
    inspect(&first, &[0, 500_000, 1_000_000, 1_500_000]);
    inspect(&second, &[0, 500_000, 1_000_000, 1_500_000]);

    let test_home = PathBuf::from(std::env::var("CLUAIZ_MEDIA_TEST_HOME")?);
    assert_eq!(PathBuf::from(std::env::var("CLUAIZ_HOME")?), test_home);
    let resolver = cluaiz_media::source::SourceResolver::from_environment(
        cluaiz_core::media::MediaSettings {
            video: settings(),
            ..Default::default()
        },
    )?;
    let request = ChatEnvelope {
        messages: vec![ChatMessage {
            role: "user".into(),
            content: vec![
                InputPart::Text {
                    text: "before".into(),
                },
                InputPart::Video {
                    source: fixtures
                        .join("red-blue-audio.mp4")
                        .to_string_lossy()
                        .into_owned(),
                    include_audio: true,
                },
                InputPart::Text {
                    text: "after".into(),
                },
            ],
        }],
        samplers: Default::default(),
        think_mode: "off".into(),
        reasoning_effort: Default::default(),
        reasoning_budget_message: None,
        chat_template_kwargs: None,
        grammar: None,
        response_format: None,
        tools: None,
    };
    let prepared = cluaiz_media::request::prepare_chat(request, &resolver, |_| Ok(())).await?;
    let parts = &prepared.messages[0].content;
    assert!(matches!(&parts[0], PreparedPart::Text { text } if text == "before"));
    for index in 0..4 {
        assert!(
            matches!(&parts[1+index*2], PreparedPart::Text { text } if text.contains(&format!("{:.3}", index as f64 / 2.0)))
        );
        assert!(matches!(&parts[2 + index * 2], PreparedPart::Image { .. }));
    }
    assert!(matches!(&parts[10], PreparedPart::Audio { .. }));
    assert!(matches!(&parts[11], PreparedPart::Text { text } if text == "after"));
    println!("REAL NATIVE VIDEO: RGB stride/colors; actual CFR/VFR timestamps; AAC through shared audio; silent/corrupt/limits/cancel/concurrent requests; ordered typed model buffers passed");
    Ok(())
}
