use anyhow::Result;
use std::f32::consts::PI;

/// Native Pure Rust Vocoder: Converts 80-channel Log-Mel Spectrogram to 24kHz PCM Audio Waveform
pub struct NativeVocoder {
    pub sample_rate: usize,
    pub n_fft: usize,
    pub hop_size: usize,
    pub num_mels: usize,
}

impl Default for NativeVocoder {
    fn default() -> Self {
        Self {
            sample_rate: 24000,
            n_fft: 1920,
            hop_size: 480,
            num_mels: 80,
        }
    }
}

impl NativeVocoder {
    pub fn new(sample_rate: usize, n_fft: usize, hop_size: usize, num_mels: usize) -> Self {
        Self {
            sample_rate,
            n_fft,
            hop_size,
            num_mels,
        }
    }

    /// Synthesize 24kHz PCM WAV audio samples from flat 80-channel mel-spectrogram [num_mels * num_frames]
    pub fn synthesize_mel_to_pcm(&self, mel_data: &[f32], num_frames: usize) -> Vec<f32> {
        if num_frames == 0 || mel_data.is_empty() {
            return Vec::new();
        }

        let total_samples = num_frames * self.hop_size + self.n_fft;
        let mut audio = vec![0.0f32; total_samples];
        let mut normalization = vec![0.0f32; total_samples];

        // Periodic Hanning window for overlap-add synthesis
        let window: Vec<f32> = (0..self.n_fft)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * (i as f32) / (self.n_fft as f32)).cos()))
            .collect();

        let num_bins = self.n_fft / 2 + 1;

        // Precompute Hz frequency for each FFT bin
        let bin_frequencies: Vec<f32> = (0..num_bins)
            .map(|b| b as f32 * (self.sample_rate as f32 / self.n_fft as f32))
            .collect();

        // Convert Hz to Slaney Mel scale
        let hz_to_mel = |hz: f32| -> f32 {
            if hz >= 1000.0 {
                15.0 + (hz / 1000.0).ln() / 0.06870054
            } else {
                3.0 * hz / 200.0
            }
        };

        let max_mel = hz_to_mel((self.sample_rate / 2) as f32);

        // Accumulated phase state per frequency bin across time frames
        let mut phase_acc = vec![0.0f32; num_bins];

        for t in 0..num_frames {
            let start_idx = t * self.num_mels;
            if start_idx >= mel_data.len() {
                break;
            }
            let end_idx = (start_idx + self.num_mels).min(mel_data.len());
            let frame_mel = &mel_data[start_idx..end_idx];

            let mut mag = vec![0.0f32; num_bins];

            for b in 0..num_bins {
                let freq_hz = bin_frequencies[b];
                let mel_val = hz_to_mel(freq_hz);
                let mel_norm = (mel_val / max_mel).clamp(0.0, 1.0);
                let mel_idx_float = mel_norm * (self.num_mels.saturating_sub(1) as f32);
                let idx_floor = mel_idx_float as usize;
                let idx_ceil = (idx_floor + 1).min(frame_mel.len().saturating_sub(1));
                let frac = mel_idx_float - idx_floor as f32;

                let m0 = if idx_floor < frame_mel.len() { frame_mel[idx_floor] } else { -4.0 };
                let m1 = if idx_ceil < frame_mel.len() { frame_mel[idx_ceil] } else { m0 };
                let log_mel = m0 + frac * (m1 - m0);

                // Denormalize mel (-4.0 to +4.0 log-energy range)
                let energy = (log_mel * 2.0).exp().max(1e-5);
                mag[b] = energy;
            }

            // Update accumulated phase for smooth continuous waveform synthesis
            let dt_sec = self.hop_size as f32 / self.sample_rate as f32;
            for b in 1..num_bins {
                phase_acc[b] = (phase_acc[b] + 2.0 * PI * bin_frequencies[b] * dt_sec) % (2.0 * PI);
            }

            let sample_offset = t * self.hop_size;

            // ISTFT Synthesis across all bins up to 128 harmonics
            let max_harmonic_bin = num_bins.min(128);
            for n in 0..self.n_fft {
                let mut sample_val = 0.0f32;
                let n_sec = n as f32 / self.sample_rate as f32;

                for k in 1..max_harmonic_bin {
                    if mag[k] > 1e-4 {
                        let phase = phase_acc[k] + 2.0 * PI * bin_frequencies[k] * n_sec;
                        sample_val += mag[k] * phase.cos();
                    }
                }

                let win_sample = sample_val * window[n];
                let out_idx = sample_offset + n;
                if out_idx < total_samples {
                    audio[out_idx] += win_sample;
                    normalization[out_idx] += window[n] * window[n];
                }
            }
        }

        // Normalize overlap-add buffer and peak scale
        let mut max_val = 0.0f32;
        for i in 0..total_samples {
            if normalization[i] > 1e-4 {
                audio[i] /= normalization[i];
            }
            if audio[i].abs() > max_val {
                max_val = audio[i].abs();
            }
        }

        if max_val > 1e-5 {
            let target_peak = 0.85f32;
            let scale = target_peak / max_val;
            for i in 0..total_samples {
                audio[i] = (audio[i] * scale).clamp(-0.95, 0.95);
            }
        }

        audio
    }

    /// Encode raw PCM f32 samples to standard 16-bit PCM WAV byte array
    pub fn encode_wav_bytes(&self, pcm_samples: &[f32]) -> Vec<u8> {
        let num_channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let byte_rate = (self.sample_rate as u32) * (num_channels as u32) * (bits_per_sample as u32 / 8);
        let block_align = num_channels * (bits_per_sample / 8);
        let data_len = (pcm_samples.len() * 2) as u32;
        let chunk_size = 36 + data_len;

        let mut header = Vec::with_capacity(44 + data_len as usize);

        // RIFF Header
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&chunk_size.to_le_bytes());
        header.extend_from_slice(b"WAVE");

        // fmt subchunk
        header.extend_from_slice(b"fmt ");
        header.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size
        header.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat (PCM)
        header.extend_from_slice(&num_channels.to_le_bytes());
        header.extend_from_slice(&(self.sample_rate as u32).to_le_bytes());
        header.extend_from_slice(&byte_rate.to_le_bytes());
        header.extend_from_slice(&block_align.to_le_bytes());
        header.extend_from_slice(&bits_per_sample.to_le_bytes());

        // data subchunk
        header.extend_from_slice(b"data");
        header.extend_from_slice(&data_len.to_le_bytes());

        // Convert f32 samples to i16 PCM bytes
        for &sample in pcm_samples {
            let s = (sample * 32767.0).max(-32768.0).min(32767.0) as i16;
            header.extend_from_slice(&s.to_le_bytes());
        }

        header
    }
}
