//! 📏 Centralized Units & Conversions for Cluaiz Engine
//! Single Source of Truth for bytes, kilobytes, megabytes, gigabytes.

pub const BYTE: u64 = 1;
pub const KIB: u64 = 1024;
pub const MIB: u64 = 1024 * 1024;
pub const GIB: u64 = 1024 * 1024 * 1024;

pub const GIB_F64: f64 = 1024.0 * 1024.0 * 1024.0;
pub const MIB_F64: f64 = 1024.0 * 1024.0;

#[inline]
pub fn bytes_to_gib(bytes: u64) -> f64 {
    bytes as f64 / GIB_F64
}

#[inline]
pub fn bytes_to_mib(bytes: u64) -> f64 {
    bytes as f64 / MIB_F64
}

#[inline]
pub fn gib_to_bytes(gib: f64) -> u64 {
    (gib * GIB_F64).round() as u64
}

#[inline]
pub fn mib_to_bytes(mib: f64) -> u64 {
    (mib * MIB_F64).round() as u64
}
