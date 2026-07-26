use super::config::AudioConfig;

pub fn build_mel_filterbank(config: &AudioConfig) -> Vec<Vec<f32>> {
    let n_mels = config.n_mels;
    let n_fft = config.n_fft;
    let n_bins = n_fft / 2 + 1;

    // Dynamic Slaney Mel Filterbank (Pure Math, zero hardcoded binary files)
    let f_min = 0.0f32;
    let f_max = (config.sample_rate / 2) as f32;

    let hz_to_mel = |hz: f32| -> f32 {
        if hz >= 1000.0 {
            15.0 + (hz / 1000.0).ln() / 0.06870054
        } else {
            3.0 * hz / 200.0
        }
    };
    let mel_to_hz = |mel: f32| -> f32 {
        if mel >= 15.0 {
            1000.0 * (0.06870054 * (mel - 15.0)).exp()
        } else {
            200.0 * mel / 3.0
        }
    };

    let mel_min = hz_to_mel(f_min);
    let mel_max = hz_to_mel(f_max);

    let mel_pts: Vec<f32> = (0..=n_mels + 1)
        .map(|i| mel_min + i as f32 * (mel_max - mel_min) / (n_mels + 1) as f32)
        .collect();
    let hz_pts: Vec<f32> = mel_pts.iter().map(|&m| mel_to_hz(m)).collect();
    let bin_pts: Vec<f32> = hz_pts.iter().map(|&h| (h * n_fft as f32 / config.sample_rate as f32)).collect();

    let mut filters = vec![vec![0.0f32; n_bins]; n_mels];
    for i in 0..n_mels {
        let b_left = bin_pts[i];
        let b_center = bin_pts[i + 1];
        let b_right = bin_pts[i + 2];

        let enorm = 2.0 / (hz_pts[i + 2] - hz_pts[i]);

        for k in 0..n_bins {
            let k_f = k as f32;
            let weight = if k_f >= b_left && k_f <= b_center {
                (k_f - b_left) / (b_center - b_left)
            } else if k_f > b_center && k_f <= b_right {
                (b_right - k_f) / (b_right - b_center)
            } else {
                0.0
            };
            filters[i][k] = weight * enorm;
        }
    }
    filters
}
