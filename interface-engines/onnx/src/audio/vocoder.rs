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

        // Hanning window for overlap-add synthesis
        let window: Vec<f32> = (0..self.n_fft)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (self.n_fft as f32 - 1.0)).cos()))
            .collect();

        let num_bins = self.n_fft / 2 + 1;

        for t in 0..num_frames {
            let sample_offset = t * self.hop_size;
            
            // Reconstruct linear spectrogram magnitudes from mel slice using logarithmic scale
            let frame_mel = &mel_data[t * self.num_mels..(t + 1) * self.num_mels.min(mel_data.len() - t * self.num_mels)];
            let mut mag = vec![0.0f32; num_bins];

            for b in 0..num_bins {
                let mel_idx = ((b as f32 / num_bins as f32) * (self.num_mels - 1) as f32) as usize;
                let val = if mel_idx < frame_mel.len() { frame_mel[mel_idx] } else { 0.0 };
                mag[b] = val.exp(); // De-log scale
            }

            // Harmonic Phase Reconstruction (Fast ISTFT approximation)
            for n in 0..self.n_fft {
                let mut sample_val = 0.0f32;
                let t_sec = (sample_offset + n) as f32 / self.sample_rate as f32;

                for k in 1..num_bins.min(64) {
                    let freq = k as f32 * (self.sample_rate as f32 / self.n_fft as f32);
                    let phase = 2.0 * PI * freq * t_sec;
                    sample_val += mag[k] * phase.cos();
                }

                let win_sample = sample_val * window[n];
                if sample_offset + n < total_samples {
                    audio[sample_offset + n] += win_sample;
                    normalization[sample_offset + n] += window[n] * window[n];
                }
            }
        }

        // Normalize overlap-add buffer and bound volume
        for i in 0..total_samples {
            if normalization[i] > 1e-5 {
                audio[i] /= normalization[i];
            }
            audio[i] = audio[i].max(-0.99).min(0.99);
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
