//! Time windows for recall. The Engine reads no time words in any language: the caller names the
//! window. The chat model turns "kal kya baat hui?" or "Monday ko" into dates in the `recall`
//! tool's arguments (it understands every language the user writes in); an API client sends the
//! same fields to `memory.search`.

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc};

/// A span of time recall keeps to: `from` inclusive, `to` exclusive
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

/// Which end of the window a value is: a bare date starts its day as `from` and ends it as `to`
#[derive(Clone, Copy, PartialEq)]
enum End {
    From,
    To,
}

/// One end of a window: `2026-10-23` (a whole day of the user's clock), `2026-10-23T21:00` (the
/// user's clock) or RFC 3339 with an offset (`2026-10-23T21:00:00+05:30`, `…Z`)
fn instant(value: &str, end: End, offset: FixedOffset) -> Result<DateTime<Utc>, String> {
    let value = value.trim();
    if let Ok(at) = DateTime::parse_from_rfc3339(value) {
        return Ok(at.with_timezone(&Utc));
    }
    let local = |naive: NaiveDateTime| offset.from_local_datetime(&naive).single().map(|at| at.with_timezone(&Utc));
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Some(at) = NaiveDateTime::parse_from_str(value, format).ok().and_then(local) {
            return Ok(at);
        }
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("'{value}' is not a time: use YYYY-MM-DD, YYYY-MM-DDTHH:MM or RFC 3339"))?;
    let date = if end == End::To { date + Duration::days(1) } else { date };
    local(date.and_time(chrono::NaiveTime::MIN)).ok_or_else(|| format!("'{value}' does not exist on this clock"))
}

/// The window named by `from`, `to` (see [`instant`]) and `within_hours` (the last that many
/// hours), with days as the user's clock (`offset`) counts them. `None` when nothing is named.
/// An open end reaches to the start of the chat or to `now`.
pub fn window_in(from: Option<&str>, to: Option<&str>, within_hours: Option<u32>, now: DateTime<Utc>, offset: FixedOffset) -> Result<Option<Window>, String> {
    let (from, to) = (from.map(str::trim).filter(|v| !v.is_empty()), to.map(str::trim).filter(|v| !v.is_empty()));
    if from.is_none() && to.is_none() && within_hours.is_none() {
        return Ok(None);
    }
    let mut window = Window { from: DateTime::<Utc>::MIN_UTC, to: now + Duration::seconds(1) };
    if let Some(from) = from {
        window.from = instant(from, End::From, offset)?;
    }
    if let Some(to) = to {
        window.to = instant(to, End::To, offset)?;
    }
    if let Some(hours) = within_hours {
        window.from = window.from.max(now - Duration::hours(hours as i64));
    }
    if window.from >= window.to {
        return Err("`from` must be before `to`".to_string());
    }
    Ok(Some(window))
}

/// [`window_in`] with days as this machine's clock counts them
pub fn window(from: Option<&str>, to: Option<&str>, within_hours: Option<u32>, now: DateTime<Utc>) -> Result<Option<Window>, String> {
    window_in(from, to, within_hours, now, chrono::Local::now().offset().fix())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Saturday 2026-10-24, 17:30 in India (12:00 UTC)
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-24T12:00:00Z").unwrap().with_timezone(&Utc)
    }
    fn ist() -> FixedOffset {
        FixedOffset::east_opt(5 * 3600 + 1800).unwrap()
    }
    fn w(from: Option<&str>, to: Option<&str>, hours: Option<u32>) -> Result<Option<(String, String)>, String> {
        window_in(from, to, hours, now(), ist()).map(|w| w.map(|w| (w.from.with_timezone(&ist()).to_rfc3339(), w.to.with_timezone(&ist()).to_rfc3339())))
    }

    #[test]
    fn test_days_are_the_users_days() {
        // "kal kya baat hui?": the model sends yesterday as from and to
        let day = w(Some("2026-10-23"), Some("2026-10-23"), None).unwrap().unwrap();
        assert_eq!(day, ("2026-10-23T00:00:00+05:30".into(), "2026-10-24T00:00:00+05:30".into()));
        // "kal raat": times of the user's clock
        let night = w(Some("2026-10-23T21:00"), Some("2026-10-24T05:00"), None).unwrap().unwrap();
        assert_eq!(night, ("2026-10-23T21:00:00+05:30".into(), "2026-10-24T05:00:00+05:30".into()));
        // An offset given is kept
        assert_eq!(w(Some("2026-10-23T00:00:00Z"), None, None).unwrap().unwrap().0, "2026-10-23T05:30:00+05:30");
    }

    #[test]
    fn test_open_ends_hours_and_mistakes() {
        assert_eq!(w(None, None, None).unwrap(), None);
        assert_eq!(w(Some(" "), None, None).unwrap(), None);
        // "3 ghante pehle": the last 3 hours
        assert_eq!(w(None, None, Some(3)).unwrap().unwrap().0, "2026-10-24T14:30:00+05:30");
        // Since a day, up to now
        assert!(w(Some("2026-10-20"), None, None).unwrap().unwrap().1.starts_with("2026-10-24T17:30:01"));
        assert!(w(Some("kal"), None, None).is_err());
        assert!(w(Some("2026-10-24"), Some("2026-10-23"), None).is_err());
    }
}
