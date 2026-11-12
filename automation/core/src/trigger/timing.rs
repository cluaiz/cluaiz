//! Time triggers: `schedule` (a cron line) and `every` (an interval). When the next time is, and
//! which planned times were missed while Automation was off or the flow disabled (`catch_up`).

use super::cron::Cron;
use super::Spec;
use crate::expr::values::{parse_time, Zone};
use crate::units::parse_duration;
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

/// At most this many runs for missed times with `catch_up: all` (*proposed*)
pub const MAX_CATCH_UP: usize = 100;

#[derive(Debug, Clone)]
pub enum Clock {
    Cron { line: Cron, zone: Zone },
    /// Every `ms`, counted from `from` (planned times, so runs do not drift)
    Every { ms: i64, from: DateTime<Utc> },
}

impl Clock {
    /// The clock of a `schedule` or `every` trigger. `since` is when counting began for an
    /// `every` without `starting` (the trigger's first activation).
    pub fn of(flow: &Value, spec: &Spec, since: DateTime<Utc>) -> Result<Clock, String> {
        match spec.kind {
            "schedule" => {
                let line = Cron::parse(spec.body().as_str().unwrap_or_default())?;
                let zone_name = spec.beside("timezone").or_else(|| flow.get("timezone")).and_then(Value::as_str);
                Ok(Clock::Cron { line, zone: Zone::parse(zone_name)? })
            }
            "every" => {
                let ms = parse_duration(spec.body().as_str().unwrap_or_default())?;
                if ms < super::MIN_INTERVAL_MS {
                    return Err("an interval is at least 1m".into());
                }
                let from = match spec.beside("starting").and_then(Value::as_str) {
                    Some(text) => parse_time(text).ok_or_else(|| format!("`{text}` is not a time"))?.with_timezone(&Utc),
                    None => since,
                };
                Ok(Clock::Every { ms, from })
            }
            other => Err(format!("`{other}` is not a time trigger")),
        }
    }

    /// The first planned time after `after` (not equal)
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Clock::Cron { line, zone } => line.next_after(after, *zone),
            Clock::Every { ms, from } => {
                if after < *from {
                    return Some(*from);
                }
                let passed = (after - *from).num_milliseconds() / ms + 1;
                Some(*from + Duration::milliseconds(passed * ms))
            }
        }
    }
}

/// The planned times after `last` and up to `now` that get a run, following `catch_up`: none
/// (`skip`), the latest one (`once`), or each of them, the latest `MAX_CATCH_UP` (`all`)
pub fn missed(clock: &Clock, last: DateTime<Utc>, now: DateTime<Utc>, catch_up: &str) -> Vec<DateTime<Utc>> {
    if catch_up == "skip" {
        return Vec::new();
    }
    let mut times = Vec::new();
    let mut after = last;
    while let Some(next) = clock.next_after(after) {
        if next > now {
            break;
        }
        times.push(next);
        if times.len() > MAX_CATCH_UP {
            times.remove(0);
        }
        after = next;
    }
    if catch_up == "once" {
        times.split_off(times.len().saturating_sub(1))
    } else {
        times
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    fn spec(item: Value) -> Spec {
        super::super::specs(&json!({ "on": [item] })).remove(0)
    }

    #[test]
    fn every_counts_from_the_planned_time() {
        let clock = Clock::of(&json!({}), &spec(json!({ "every": "15m", "starting": "2026-11-11T08:00:00+05:30" })), Utc::now()).unwrap();
        assert_eq!(clock.next_after(at("2026-11-10T00:00:00Z")), Some(at("2026-11-11T08:00:00+05:30")));
        assert_eq!(clock.next_after(at("2026-11-11T08:00:00+05:30")), Some(at("2026-11-11T08:15:00+05:30")));
        assert_eq!(clock.next_after(at("2026-11-11T08:07:10+05:30")), Some(at("2026-11-11T08:15:00+05:30")));
        assert!(Clock::of(&json!({}), &spec(json!({ "every": "10s" })), Utc::now()).is_err());
    }

    #[test]
    fn catch_up_skip_once_all() {
        let clock = Clock::of(&json!({ "timezone": "UTC" }), &spec(json!({ "schedule": "0 * * * *" })), Utc::now()).unwrap();
        let (last, now) = (at("2026-11-10T08:00:00Z"), at("2026-11-10T11:30:00Z"));
        assert!(missed(&clock, last, now, "skip").is_empty());
        assert_eq!(missed(&clock, last, now, "once"), vec![at("2026-11-10T11:00:00Z")]);
        assert_eq!(missed(&clock, last, now, "all"), vec![at("2026-11-10T09:00:00Z"), at("2026-11-10T10:00:00Z"), at("2026-11-10T11:00:00Z")]);
        let minutes = Clock::of(&json!({}), &spec(json!({ "every": "1m", "starting": "2026-11-10T00:00:00Z" })), Utc::now()).unwrap();
        let all = missed(&minutes, at("2026-11-10T00:00:00Z"), at("2026-11-10T10:00:00Z"), "all");
        assert_eq!(all.len(), MAX_CATCH_UP);
        assert_eq!(all.last(), Some(&at("2026-11-10T10:00:00Z")));
        assert!(missed(&clock, now, now, "all").is_empty());
    }

    #[test]
    fn the_trigger_timezone_wins() {
        let flow = json!({ "timezone": "UTC" });
        let clock = Clock::of(&flow, &spec(json!({ "schedule": "0 9 * * *", "timezone": "Asia/Kolkata" })), Utc::now()).unwrap();
        assert_eq!(clock.next_after(at("2026-11-10T00:00:00Z")), Some(at("2026-11-10T09:00:00+05:30")));
    }
}
