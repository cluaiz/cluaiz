//! Adapted from Edge0-AI/Audio8_TTS@07e40f5 (Apache-2.0); modified native Rust runtime.
use anyhow::{bail, Context, Result};
use cluaiz_core::media_limits::MediaLimits;
use half::f16;
use ort::{
    session::{Session, SessionInputValue, SessionOutputs},
    value::{TensorElementType, TensorRef},
};

/// Fixed-capacity KV with outputs containing only the positions evaluated in this step.
pub(super) struct Caches {
    shape: [usize; 4],
    values: Vec<Vec<f16>>,
}

impl Caches {
    pub fn bytes(layers: usize, heads: usize, capacity: usize, width: usize) -> Result<usize> {
        [
            layers,
            2,
            heads,
            capacity,
            width,
            std::mem::size_of::<f16>(),
        ]
        .into_iter()
        .try_fold(1usize, |a, b| a.checked_mul(b))
        .context("Audio8 cache size overflow")
    }

    pub fn new(
        layers: usize,
        heads: usize,
        capacity: usize,
        width: usize,
        limits: MediaLimits,
    ) -> Result<Self> {
        limits.check_decoded_bytes(u64::try_from(Self::bytes(layers, heads, capacity, width)?)?)?;
        let count = heads
            .checked_mul(capacity)
            .and_then(|n| n.checked_mul(width))
            .context("Cache size overflow")?;
        let mut values = Vec::with_capacity(layers * 2);
        for _ in 0..layers * 2 {
            values.push(vec![f16::ZERO; count]);
        }
        Ok(Self {
            shape: [1, heads, capacity, width],
            values,
        })
    }

    pub fn validate(&self, session: &Session) -> Result<()> {
        for name in self.names() {
            let input = session
                .inputs()
                .iter()
                .find(|i| i.name() == name)
                .context("Audio8 cache input missing")?;
            crate::media::validate_tensor(
                input.dtype(),
                TensorElementType::Float16,
                &self.shape.map(|n| n as i64),
            )?;
        }
        Ok(())
    }

    fn names(&self) -> impl Iterator<Item = String> + '_ {
        (0..self.values.len()).map(|i| {
            format!(
                "cache_{}_{}",
                if i % 2 == 0 { "key" } else { "value" },
                i / 2
            )
        })
    }

    pub fn feeds(&self) -> Result<Vec<(String, SessionInputValue<'_>)>> {
        self.names()
            .zip(&self.values)
            .map(|(name, value)| {
                Ok((
                    name,
                    TensorRef::from_array_view((self.shape, value.as_slice()))?.into(),
                ))
            })
            .collect()
    }

    pub fn reset(&mut self) {
        for v in &mut self.values {
            v.fill(f16::ZERO);
        }
    }

    pub fn update(
        &mut self,
        outputs: &SessionOutputs<'_>,
        skip: usize,
        positions: &[i64],
    ) -> Result<()> {
        if outputs.len() != skip + self.values.len() {
            bail!("Audio8 cache output count mismatch");
        }
        let [_, heads, capacity, width] = self.shape;
        for (i, values) in self.values.iter_mut().enumerate() {
            let (shape, delta) = outputs[i + skip].try_extract_tensor::<f16>()?;
            if shape.as_ref() != [1, heads as i64, positions.len() as i64, width as i64] {
                bail!("Audio8 cache delta shape mismatch: {shape:?}");
            }
            for (t, &position) in positions.iter().enumerate() {
                let p = usize::try_from(position)?;
                if p >= capacity {
                    bail!("Audio8 cache position exceeds capacity");
                }
                for h in 0..heads {
                    let dst = (h * capacity + p) * width;
                    let src = (h * positions.len() + t) * width;
                    values[dst..dst + width].copy_from_slice(&delta[src..src + width]);
                }
            }
        }
        Ok(())
    }
}
