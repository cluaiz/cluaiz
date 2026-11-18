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

/// GiB as the gigabytes Hugging Face shows (10^9 bytes), for sizes a person compares with the model page
#[inline]
pub fn gib_to_gb(gib: f64) -> f64 {
    gib * GIB_F64 / 1e9
}

/// Bytes as the gigabytes Hugging Face and `cluaiz pull` show for a file (10^9 bytes)
#[inline]
pub fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / 1e9
}

/// A count with thousands separators (16384 -> "16,384")
pub fn group_digits(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[inline]
pub fn gib_to_bytes(gib: f64) -> u64 {
    (gib * GIB_F64).round() as u64
}

#[inline]
pub fn mib_to_bytes(mib: f64) -> u64 {
    (mib * MIB_F64).round() as u64
}

#[cfg(test)]
mod tests {
    #[test]
    fn group_digits_puts_a_comma_every_three_digits() {
        assert_eq!(super::group_digits(0), "0");
        assert_eq!(super::group_digits(999), "999");
        assert_eq!(super::group_digits(4096), "4,096");
        assert_eq!(super::group_digits(262144), "262,144");
        assert_eq!(super::group_digits(1234567), "1,234,567");
    }
}
