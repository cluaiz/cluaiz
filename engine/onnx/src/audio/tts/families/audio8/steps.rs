//! Adapted from Edge0-AI/Audio8_TTS@07e40f5 (Apache-2.0); modified native Rust runtime.
use super::{cache::Caches, package::Manifest};
use anyhow::{bail, Result};
use half::f16;
use ort::{
    session::Session,
    value::{DynValue, TensorRef},
};

pub(super) fn slow(
    session: &mut Session,
    codes: &[i64],
    positions: &[i64],
    cache: &mut Caches,
    m: &Manifest,
) -> Result<(Vec<f64>, Vec<f16>)> {
    let outputs = {
        let mut feeds = cache.feeds()?;
        feeds.push((
            "codes".into(),
            TensorRef::from_array_view(([1, m.num_codebooks + 1, positions.len()], codes))?.into(),
        ));
        feeds.push((
            "input_pos".into(),
            TensorRef::from_array_view(([positions.len()], positions))?.into(),
        ));
        session.run(feeds)?
    };
    cache.update(&outputs, 2, positions)?;
    let logits = last_logits(&outputs[0], positions.len(), m.codebook_size + 1)?;
    let (shape, hidden) = outputs[1].try_extract_tensor::<f16>()?;
    if !last_step_shape(shape.as_ref(), positions.len(), m.fast_dim) {
        bail!(
            "Audio8 slow hidden shape mismatch: {shape:?}; input positions {} and hidden width {}",
            positions.len(),
            m.fast_dim
        );
    }
    Ok((logits, hidden[hidden.len() - m.fast_dim..].to_vec()))
}

pub(super) fn fast(
    session: &mut Session,
    hidden: &[f16],
    token: i64,
    use_hidden: bool,
    position: usize,
    cache: &mut Caches,
    m: &Manifest,
) -> Result<Vec<f64>> {
    let token = [token];
    let use_hidden = [use_hidden];
    let position = [position as i64];
    let outputs = {
        let mut feeds = cache.feeds()?;
        feeds.push((
            "slow_hidden".into(),
            TensorRef::from_array_view(([1, 1, m.fast_dim], hidden))?.into(),
        ));
        feeds.push((
            "token_id".into(),
            TensorRef::from_array_view(([1, 1], token.as_slice()))?.into(),
        ));
        feeds.push((
            "use_slow_hidden".into(),
            TensorRef::from_array_view(([1], use_hidden.as_slice()))?.into(),
        ));
        feeds.push((
            "input_pos".into(),
            TensorRef::from_array_view(([1], position.as_slice()))?.into(),
        ));
        session.run(feeds)?
    };
    cache.update(&outputs, 1, &position)?;
    last_logits(&outputs[0], 1, m.codebook_size)
}

fn last_logits(value: &DynValue, time: usize, width: usize) -> Result<Vec<f64>> {
    if let Ok((shape, data)) = value.try_extract_tensor::<f16>() {
        if !last_step_shape(shape.as_ref(), time, width) {
            bail!("Audio8 logits shape mismatch: {shape:?}; input time {time} and logits width {width}");
        }
        return Ok(data[data.len() - width..]
            .iter()
            .map(|v| v.to_f64())
            .collect());
    }
    let (shape, data) = value.try_extract_tensor::<f32>()?;
    if !last_step_shape(shape.as_ref(), time, width) {
        bail!(
            "Audio8 logits shape mismatch: {shape:?}; input time {time} and logits width {width}"
        );
    }
    Ok(data[data.len() - width..]
        .iter()
        .map(|&v| f64::from(v))
        .collect())
}

/// The inspected export returns only its final logit/hidden step even during a multi-token
/// prefill ([1,1,4097] for 33 input positions). Other exports may return every evaluated step.
fn last_step_shape(shape: &[i64], input_time: usize, width: usize) -> bool {
    shape.len() == 3
        && shape[0] == 1
        && shape[2] == width as i64
        && (shape[1] == 1 || shape[1] == input_time as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_prefill_can_return_only_final_logits_and_hidden() {
        assert!(last_step_shape(&[1, 1, 4097], 33, 4097));
        assert!(last_step_shape(&[1, 33, 4097], 33, 4097));
        assert!(!last_step_shape(&[1, 2, 4097], 33, 4097));
        assert!(!last_step_shape(&[2, 1, 4097], 33, 4097));
        assert!(!last_step_shape(&[1, 1, 4096], 33, 4097));
    }
}
