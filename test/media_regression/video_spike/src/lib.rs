//! Build/packaging acceptance only. No production CLI or second decoder owner.
#[cfg(all(test, feature = "native-sdk"))]
mod tests {
    use anyhow::{bail, Context, Result};
    use ffmpeg_next as ffmpeg;

    #[test]
    fn test_packaged_native_mp4_decodes_timestamped_rgb_rows() -> Result<()> {
        let source = std::env::var("CLUAIZ_VIDEO_SPIKE_FILE")?;
        ffmpeg::init()?;
        let mut input = ffmpeg::format::input(&source)?;
        let stream = input
            .streams()
            .best(ffmpeg::media::Type::Video)
            .context("Video track")?;
        let index = stream.index();
        let time_base = stream.time_base();
        let mut decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?
            .decoder()
            .video()?;
        let mut scaler = ffmpeg::software::scaling::context::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg::format::Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            ffmpeg::software::scaling::flag::Flags::BILINEAR,
        )?;
        assert_eq!((decoder.width(), decoder.height()), (34, 26));
        let mut frames = Vec::new();
        let mut receive = |decoder: &mut ffmpeg::decoder::Video| -> Result<()> {
            loop {
                let mut frame = ffmpeg::frame::Video::empty();
                match decoder.receive_frame(&mut frame) {
                    Ok(()) => {}
                    Err(ffmpeg::Error::Eof) => break,
                    Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => break,
                    Err(error) => return Err(error.into()),
                }
                let timestamp = frame.timestamp().context("Frame timestamp")?;
                let mut rgb = ffmpeg::frame::Video::empty();
                scaler.run(&frame, &mut rgb)?;
                let row_bytes = rgb.width() as usize * 3;
                let mut pixels = Vec::new();
                for y in 0..rgb.height() as usize {
                    let start = y.checked_mul(rgb.stride(0)).context("Row offset")?;
                    pixels.extend_from_slice(
                        rgb.data(0)
                            .get(start..start + row_bytes)
                            .context("RGB row")?,
                    );
                }
                assert_eq!(pixels.len(), 34 * 26 * 3);
                let seconds = timestamp as f64 * f64::from(time_base);
                frames.push((seconds, pixels));
            }
            Ok(())
        };
        loop {
            let mut packet = ffmpeg::Packet::empty();
            match packet.read(&mut input) {
                Ok(()) if packet.stream() == index => {
                    decoder.send_packet(&packet)?;
                    receive(&mut decoder)?;
                }
                Ok(()) => {}
                Err(ffmpeg::Error::Eof) => break,
                Err(error) => return Err(error.into()),
            }
        }
        decoder.send_eof()?;
        receive(&mut decoder)?;
        if frames.len() != 4 {
            bail!("Expected four genuine frames, got {}", frames.len());
        }
        for (i, (seconds, pixels)) in frames.iter().enumerate() {
            assert!((seconds - i as f64 / 2.0).abs() < 0.01);
            let [r, g, b] = [pixels[0], pixels[1], pixels[2]];
            if i < 2 {
                assert!(r > 220 && g < 45 && b < 45);
            } else {
                assert!(b > 220 && r < 45 && g < 45);
            }
        }
        println!("REAL VIDEO SDK: {}-{}; four H264 RGB frames at 0/0.5/1/1.5 seconds; packed odd stride passed",std::env::consts::OS,std::env::consts::ARCH);
        Ok(())
    }
}

#[cfg(test)]
mod lazy_loader;
