//! Durations (`500ms`, `30s`, `1h30m`, `-7d`) and sizes (`512KB`, `4MB`), as in
//! docs/automation/limits.md#durations-and-sizes.

const UNITS_MS: [(&str, i64); 5] = [("ms", 1), ("s", 1_000), ("m", 60_000), ("h", 3_600_000), ("d", 86_400_000)];
const SIZE_UNITS: [(&str, u64); 4] = [("KB", 1 << 10), ("MB", 1 << 20), ("GB", 1 << 30), ("B", 1)];

/// Milliseconds; a leading `-` gives a negative duration (for `add_time`)
pub fn parse_duration(text: &str) -> Result<i64, String> {
    let text = text.trim();
    let (sign, body) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text),
    };
    if body.is_empty() {
        return Err(format!("`{text}` is not a duration such as 30s, 10m or 1h30m"));
    }
    let mut total: i64 = 0;
    let mut rest = body;
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if digits == 0 {
            return Err(format!("`{text}` is not a duration such as 30s, 10m or 1h30m"));
        }
        let number: i64 = rest[..digits].parse().map_err(|_| format!("`{text}` is too large"))?;
        rest = &rest[digits..];
        // "ms" before "m", so the longest unit is matched first
        let (unit, factor) = UNITS_MS
            .iter()
            .find(|(unit, _)| rest.starts_with(unit) && !(*unit == "m" && rest.starts_with("ms")))
            .ok_or_else(|| format!("`{text}` needs a unit: ms, s, m, h or d"))?;
        total = number
            .checked_mul(*factor)
            .and_then(|ms| total.checked_add(ms))
            .ok_or_else(|| format!("`{text}` is too large"))?;
        rest = &rest[unit.len()..];
    }
    Ok(sign * total)
}

/// Bytes, 1024-based
pub fn parse_size(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let digits = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    let number: u64 = text[..digits].parse().map_err(|_| format!("`{text}` is not a size such as 512KB or 4MB"))?;
    let unit = &text[digits..];
    let factor = SIZE_UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map(|(_, factor)| *factor)
        .ok_or_else(|| format!("`{text}` needs a unit: B, KB, MB or GB"))?;
    number.checked_mul(factor).ok_or_else(|| format!("`{text}` is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("500ms"), Ok(500));
        assert_eq!(parse_duration("30s"), Ok(30_000));
        assert_eq!(parse_duration("10m"), Ok(600_000));
        assert_eq!(parse_duration("1h30m"), Ok(5_400_000));
        assert_eq!(parse_duration("-7d"), Ok(-604_800_000));
        for bad in ["", "10", "m", "5x", "1h 30m", "-"] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("512KB"), Ok(512 * 1024));
        assert_eq!(parse_size("4MB"), Ok(4 << 20));
        assert_eq!(parse_size("10B"), Ok(10));
        for bad in ["4", "MB", "4mb", "1.5MB"] {
            assert!(parse_size(bad).is_err(), "{bad}");
        }
    }
}
