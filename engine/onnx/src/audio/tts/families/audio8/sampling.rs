//! Adapted from Edge0-AI/Audio8_TTS@07e40f5 (Apache-2.0); modified native Rust runtime.
//! The upstream nucleus/rank filter is applied before temperature, then exponential-race sampling.
use anyhow::{bail, Context, Result};
use rand::Rng;

pub(super) fn sample(
    logits: &[f64],
    temperature: f64,
    top_p: f64,
    top_k: usize,
    rng: &mut impl Rng,
) -> Result<usize> {
    if logits.is_empty()
        || logits.iter().any(|x| x.is_nan() || *x == f64::INFINITY)
        || !temperature.is_finite()
        || temperature <= 0.0
        || !(0.0..=1.0).contains(&top_p)
        || top_k == 0
    {
        bail!("Invalid Audio8 sampling input");
    }
    let mut order: Vec<_> = (0..logits.len()).collect();
    order.sort_by(|&a, &b| logits[b].total_cmp(&logits[a]));
    let maximum = logits[order[0]];
    if !maximum.is_finite() {
        bail!("Audio8 logits have no finite candidate");
    }
    let sum: f64 = logits.iter().map(|x| (x - maximum).exp()).sum();
    let mut cumulative = 0.0;
    let mut best: Option<(usize, f64)> = None;
    for (rank, &id) in order.iter().enumerate() {
        cumulative += (logits[id] - maximum).exp() / sum;
        if rank != 0 && (rank >= top_k || cumulative > top_p) {
            continue;
        }
        let weight = ((logits[id] - maximum) / temperature).exp();
        let score = weight / -rng.gen::<f64>().max(f64::MIN_POSITIVE).ln();
        if best.is_none_or(|(_, previous)| score > previous) {
            best = Some((id, score));
        }
    }
    Ok(best.context("No Audio8 sampling candidate")?.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};
    #[test]
    fn test_sampling_rank_filter_and_invalid_logits() -> Result<()> {
        let mut rng = StdRng::seed_from_u64(9);
        for _ in 0..100 {
            assert_eq!(sample(&[-2.0, 3.0, 1.0], 0.7, 0.9, 1, &mut rng)?, 1);
        }
        assert!(sample(&[f64::NEG_INFINITY], 1.0, 0.9, 50, &mut rng).is_err());
        assert!(sample(&[f64::NAN], 1.0, 0.9, 50, &mut rng).is_err());
        Ok(())
    }
}
