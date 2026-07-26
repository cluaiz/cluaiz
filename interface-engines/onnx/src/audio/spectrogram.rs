use super::config::AudioConfig;
use super::mel_bank::build_mel_filterbank;
use std::f32::consts::PI;

pub fn compute_log_mel_spectrogram(samples: &[f32], config: &AudioConfig) -> Vec<f32> {
    let target_len = config.max_samples;
    let pad_len = config.n_fft / 2;

    let mut s30 = vec![0.0f32; target_len];
    let copy_len = samples.len().min(target_len);
    s30[..copy_len].copy_from_slice(&samples[..copy_len]);

    let total_len = target_len + 2 * pad_len;
    let mut padded = vec![0.0f32; total_len];
    padded[pad_len..pad_len + target_len].copy_from_slice(&s30);

    for i in 0..pad_len {
        padded[pad_len - 1 - i] = s30.get(i + 1).copied().unwrap_or(0.0);
    }
    for i in 0..pad_len {
        let idx = target_len.saturating_sub(2 + i);
        padded[pad_len + target_len + i] = s30.get(idx).copied().unwrap_or(0.0);
    }

    let window: Vec<f32> = (0..config.n_fft)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / config.n_fft as f32).cos()))
        .collect();

    let filters = build_mel_filterbank(config);
    let n_bins = config.n_fft / 2 + 1;
    let mut mel_matrix = vec![vec![0.0f32; config.max_frames]; config.n_mels];

    let mut planner = rustfft::FftPlanner::<f32>::new();
    let fft_plan = planner.plan_fft_forward(config.n_fft);

    for frame_idx in 0..config.max_frames {
        let start = frame_idx * config.hop_length;
        let mut frame = vec![0.0f32; config.n_fft];
        for i in 0..config.n_fft {
            let s = if start + i < padded.len() { padded[start + i] } else { 0.0 };
            frame[i] = s * window[i];
        }

        let power = fft_power_spectrum_with_plan(&frame, config.n_fft, &*fft_plan);

        for mel_idx in 0..config.n_mels {
            let mut energy = 0.0f32;
            for bin in 0..n_bins {
                energy += filters[mel_idx][bin] * power[bin];
            }
            mel_matrix[mel_idx][frame_idx] = energy.max(1e-10).log10();
        }
    }

    let global_max = mel_matrix.iter()
        .flat_map(|r| r.iter())
        .cloned()
        .fold(f32::NEG_INFINITY, f32::max);

    let mut flat = Vec::with_capacity(config.n_mels * config.max_frames);
    for mel_idx in 0..config.n_mels {
        for frame_idx in 0..config.max_frames {
            let v = mel_matrix[mel_idx][frame_idx];
            flat.push(((v.max(global_max - 8.0)) + 4.0) / 4.0);
        }
    }
    flat
}

fn fft_power_spectrum_with_plan(
    frame: &[f32],
    n_fft: usize,
    fft_plan: &dyn rustfft::Fft<f32>,
) -> Vec<f32> {
    let mut buffer: Vec<rustfft::num_complex::Complex<f32>> = frame
        .iter()
        .map(|&s| rustfft::num_complex::Complex::new(s, 0.0))
        .collect();

    fft_plan.process(&mut buffer);

    let n_bins = n_fft / 2 + 1;
    let mut power = Vec::with_capacity(n_bins);
    for bin in buffer.iter().take(n_bins) {
        power.push(bin.norm_sqr());
    }
    power
}
