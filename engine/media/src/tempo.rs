//! Streaming, pitch-preserving speech tempo using Sonic (Apache-2.0, via MIT bindings).
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use sonic_rs_sys as sonic;
use std::ptr::NonNull;

pub const SPEED_RANGE: (f32, f32) = (0.25, 4.0);

pub struct Tempo {
    stream: Option<NonNull<sonic::sonicStreamStruct>>,
    limits: MediaLimits,
    output_samples: usize,
    finished: bool,
    speed: f32,
    latency_samples: usize,
    pending: Vec<f32>,
}

impl Tempo {
    pub fn new(sample_rate: u32, speed: f32, limits: MediaLimits) -> Result<Self> {
        let rate = i32::try_from(sample_rate).context("Tempo sample rate exceeds native range")?;
        if rate <= 0 || !speed.is_finite() || !(SPEED_RANGE.0..=SPEED_RANGE.1).contains(&speed) {
            bail!("Tempo requires a positive sample rate and speed between {} and {}", SPEED_RANGE.0, SPEED_RANGE.1);
        }
        let latency_samples = sample_rate as usize / sonic::SONIC_MIN_PITCH as usize * 2;
        let stream = if speed == 1.0 { None } else {
            if rate < sonic::SONIC_MAX_PITCH as i32 { bail!("Sample rate is below Sonic's pitch-analysis range"); }
            // Conservative initial native-buffer preflight; later growth is checked at each write/read.
            limits.check_decoded_bytes(u64::try_from(latency_samples.checked_mul(16).context("Tempo workspace overflow")?)?)?;
            // SAFETY: validated rate, mono; the returned stream is exclusively owned until Drop.
            let stream = NonNull::new(unsafe { sonic::sonicCreateStream(rate, 1) })
                .context("Sonic could not allocate a tempo stream")?;
            // SAFETY: owned live stream, validated speed; pitch/rate retain Sonic's defaults.
            unsafe { sonic::sonicSetSpeed(stream.as_ptr(), speed) };
            Some(stream)
        };
        Ok(Self { stream, limits, output_samples: 0, finished: false, speed, latency_samples, pending: Vec::new() })
    }

    pub fn write(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        if self.finished { bail!("Cannot write to a finished tempo stream"); }
        if samples.iter().any(|sample| !sample.is_finite()) { bail!("Tempo PCM contains nonfinite samples"); }
        i32::try_from(samples.len()).context("Tempo input exceeds native range")?;
        let predicted = if self.stream.is_some() {
            (samples.len() as f64 / self.speed as f64).ceil() as usize + self.latency_samples
        } else { samples.len() };
        self.check_budget(samples.len(), predicted)?;
        if samples.is_empty() { return Ok(Vec::new()); }
        if self.stream.is_none() {
            self.output_samples = self.output_samples.checked_add(samples.len()).context("Tempo output overflow")?;
            return Ok(samples.to_vec());
        }
        // Sonic's decisions vary with write partitioning. Feed a stable quantum derived from its
        // own two-pitch-period analysis window, independent of the caller's network/native chunks.
        self.pending.extend_from_slice(samples);
        let mut buffered = std::mem::take(&mut self.pending);
        let end = buffered.len() / self.latency_samples * self.latency_samples;
        let mut output = Vec::new();
        for part in buffered[..end].chunks_mut(self.latency_samples) {
            output.extend(self.process(part)?);
        }
        self.pending.extend_from_slice(&buffered[end..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<f32>> {
        if self.finished { return Ok(Vec::new()); }
        self.finished = true;
        let Some(stream) = self.stream else { return Ok(Vec::new()); };
        let mut pending = std::mem::take(&mut self.pending);
        let mut output = if pending.is_empty() { Vec::new() } else { self.process(&mut pending)? };
        // SAFETY: exclusively owned live stream; flush emits its pending tail once.
        if unsafe { sonic::sonicFlushStream(stream.as_ptr()) } == 0 { bail!("Sonic failed to flush speech"); }
        output.extend(self.read()?);
        Ok(output)
    }

    fn process(&mut self, input: &mut [f32]) -> Result<Vec<f32>> {
        let stream = self.stream.context("Tempo stream missing")?;
        let count = i32::try_from(input.len())?;
        // SAFETY: owned stream and live writable count-sample buffer; Sonic copies before returning.
        if unsafe { sonic::sonicWriteFloatToStream(stream.as_ptr(), input.as_mut_ptr(), count) } == 0 {
            bail!("Sonic failed to buffer tempo input");
        }
        self.read()
    }

    fn check_budget(&self, input: usize, output: usize) -> Result<()> {
        let samples = input.checked_add(self.pending.len()).and_then(|n| n.checked_mul(2))
            .and_then(|n| self.output_samples.checked_add(output)?.checked_add(n))
            .context("Tempo buffer budget overflow")?;
        self.limits.check_decoded_bytes(u64::try_from(samples.checked_mul(4).context("Tempo byte count overflow")?)?)?;
        Ok(())
    }

    fn read(&mut self) -> Result<Vec<f32>> {
        let stream = self.stream.context("Tempo stream missing")?;
        // SAFETY: live stream, a read-only count query.
        let count = unsafe { sonic::sonicSamplesAvailable(stream.as_ptr()) };
        let count = usize::try_from(count).context("Sonic returned a negative sample count")?;
        self.check_budget(0, count)?;
        let mut output = vec![0.0; count];
        // SAFETY: output owns count writable F32 samples and the queried native count fits i32.
        let read = unsafe { sonic::sonicReadFloatFromStream(stream.as_ptr(), output.as_mut_ptr(), count as i32) };
        if read < 0 || read as usize > count { bail!("Sonic returned an invalid output count"); }
        output.truncate(read as usize);
        self.output_samples = self.output_samples.checked_add(output.len()).context("Tempo output count overflow")?;
        if output.iter().any(|sample| !sample.is_finite()) { bail!("Sonic produced nonfinite PCM"); }
        Ok(output)
    }
}

impl Drop for Tempo {
    fn drop(&mut self) {
        if let Some(stream) = self.stream {
            // SAFETY: this owner frees its native stream exactly once, after all synchronous calls end.
            unsafe { sonic::sonicDestroyStream(stream.as_ptr()) };
        }
    }
}

#[cfg(test)]
#[path = "tempo_tests.rs"]
mod tests;
