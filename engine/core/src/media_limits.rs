//! Caller-configured byte limits to check before reading or allocating media buffers.

use crate::units::MIB;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

/// Default source ceiling; callers can override it through media settings.
pub const DEFAULT_MEDIA_INPUT_BYTES: NonZeroU64 = match NonZeroU64::new(64 * MIB) {
    Some(value) => value,
    None => NonZeroU64::MIN,
};
/// Default decoded ceiling, independent of encoded compression ratio.
pub const DEFAULT_MEDIA_DECODED_BYTES: NonZeroU64 = match NonZeroU64::new(256 * MIB) {
    Some(value) => value,
    None => NonZeroU64::MIN,
};

/// The resource whose byte limit was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaLimitKind {
    EncodedInput,
    DecodedData,
}

/// Invalid configuration or an operation that would exceed its configured budget.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MediaLimitError {
    #[error("Media byte limits must be non-zero")]
    ZeroLimit,
    #[error("{kind:?} requires {actual} bytes; configured limit is {limit} bytes")]
    Exceeded {
        kind: MediaLimitKind,
        actual: u64,
        limit: u64,
    },
}

/// Byte ceilings supplied by the caller/configuration; no model-specific sizes are assumed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MediaLimits {
    max_input_bytes: NonZeroU64,
    max_decoded_bytes: NonZeroU64,
}

impl Default for MediaLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: DEFAULT_MEDIA_INPUT_BYTES,
            max_decoded_bytes: DEFAULT_MEDIA_DECODED_BYTES,
        }
    }
}

impl MediaLimits {
    /// Creates a policy. Zero means invalid configuration, not unlimited allocation.
    pub fn new(max_input_bytes: u64, max_decoded_bytes: u64) -> Result<Self, MediaLimitError> {
        Ok(Self {
            max_input_bytes: NonZeroU64::new(max_input_bytes).ok_or(MediaLimitError::ZeroLimit)?,
            max_decoded_bytes: NonZeroU64::new(max_decoded_bytes)
                .ok_or(MediaLimitError::ZeroLimit)?,
        })
    }

    /// Maximum encoded input size, including bytes accumulated while streaming a source.
    pub fn max_input_bytes(&self) -> u64 {
        self.max_input_bytes.get()
    }

    /// Maximum decoded byte count; consumers must include simultaneously retained buffers.
    pub fn max_decoded_bytes(&self) -> u64 {
        self.max_decoded_bytes.get()
    }

    /// Checks a file size or running source byte count before further reads.
    pub fn check_input_bytes(&self, actual: u64) -> Result<(), MediaLimitError> {
        Self::check(MediaLimitKind::EncodedInput, actual, self.max_input_bytes)
    }

    /// Checks the planned decoded byte count before allocation. Does not allocate or count buffers.
    pub fn check_decoded_bytes(&self, actual: u64) -> Result<(), MediaLimitError> {
        Self::check(MediaLimitKind::DecodedData, actual, self.max_decoded_bytes)
    }

    fn check(kind: MediaLimitKind, actual: u64, limit: NonZeroU64) -> Result<(), MediaLimitError> {
        if actual > limit.get() {
            return Err(MediaLimitError::Exceeded {
                kind,
                actual,
                limit: limit.get(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limits_exact_boundary_accepts_and_next_byte_rejects() {
        let limits = MediaLimits::new(4, 12).unwrap();
        assert_eq!(limits.check_input_bytes(4), Ok(()));
        assert_eq!(limits.check_decoded_bytes(12), Ok(()));
        assert_eq!(
            limits.check_input_bytes(5),
            Err(MediaLimitError::Exceeded {
                kind: MediaLimitKind::EncodedInput,
                actual: 5,
                limit: 4
            })
        );
        assert_eq!(
            limits.check_decoded_bytes(13),
            Err(MediaLimitError::Exceeded {
                kind: MediaLimitKind::DecodedData,
                actual: 13,
                limit: 12
            })
        );
    }

    #[test]
    fn test_limits_zero_configuration_rejects() {
        for (input, decoded) in [(0, 1), (1, 0), (0, 0)] {
            assert_eq!(
                MediaLimits::new(input, decoded),
                Err(MediaLimitError::ZeroLimit)
            );
        }
        assert!(serde_json::from_str::<MediaLimits>(
            r#"{"max_input_bytes":0,"max_decoded_bytes":1}"#
        )
        .is_err());
    }

    #[test]
    fn test_limits_json_round_trip_preserves_caller_policy() {
        let limits = MediaLimits::new(7, 19).unwrap();
        let json = serde_json::to_string(&limits).unwrap();
        assert_eq!(serde_json::from_str::<MediaLimits>(&json).unwrap(), limits);
        assert_eq!(
            (limits.max_input_bytes(), limits.max_decoded_bytes()),
            (7, 19)
        );
    }

    #[test]
    fn test_limits_empty_and_maximum_counts_do_not_overflow() {
        let limits = MediaLimits::new(u64::MAX, u64::MAX).unwrap();
        assert_eq!(limits.check_input_bytes(0), Ok(()));
        assert_eq!(limits.check_decoded_bytes(u64::MAX), Ok(()));
    }
}
