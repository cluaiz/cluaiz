//! Cron lines of `schedule` triggers: five fields, and the next time one names in a timezone
//! (docs/automation/triggers.md#schedule).

use crate::expr::values::Zone;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Timelike, Utc};

const DAY_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
const MONTH_NAMES: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
/// How far ahead a time is looked for: a line such as `0 0 29 2 1` may wait years
const LOOK_AHEAD_DAYS: i64 = 366 * 8;

/// One field: the allowed values as bits, and whether it was written starting with `*`
#[derive(Debug, Clone, Copy, PartialEq)]
struct Field {
    bits: u64,
    star: bool,
}

impl Field {
    fn has(&self, n: u32) -> bool {
        self.bits & (1 << n) != 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cron {
    minute: Field,
    hour: Field,
    day: Field,
    month: Field,
    weekday: Field,
}

fn number(text: &str, low: u32, high: u32, names: &[&str], first_name: u32, place: usize) -> Result<u32, String> {
    let value = text.parse::<u32>().ok().or_else(|| names.iter().position(|n| n.eq_ignore_ascii_case(text)).map(|p| p as u32 + first_name));
    match value {
        Some(n) if (low..=high).contains(&n) => Ok(n),
        _ => Err(format!("`{text}` is not allowed in field {place} (from {low} to {high})")),
    }
}

fn field(text: &str, place: usize, low: u32, high: u32, names: &[&str], first_name: u32) -> Result<Field, String> {
    let mut bits = 0u64;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => match step.parse::<u32>() {
                Ok(step) if step > 0 => (range, step),
                _ => return Err(format!("`{part}`: the step after / must be a whole number above 0")),
            },
            None => (part, 1),
        };
        let (from, to) = match range {
            "*" => (low, high),
            _ => match range.split_once('-') {
                Some((a, b)) => (number(a, low, high, names, first_name, place)?, number(b, low, high, names, first_name, place)?),
                // `5/15`: from 5 to the end, every 15
                None if step > 1 => (number(range, low, high, names, first_name, place)?, high),
                None => {
                    let n = number(range, low, high, names, first_name, place)?;
                    (n, n)
                }
            },
        };
        if from > to {
            return Err(format!("`{range}`: a range goes from the smaller value to the larger"));
        }
        let mut n = from;
        while n <= to {
            bits |= 1 << n;
            n += step;
        }
    }
    Ok(Field { bits, star: text.starts_with('*') })
}

impl Cron {
    /// Minute, hour, day of month, month, day of week: `*`, lists, ranges, steps and names
    pub fn parse(line: &str) -> Result<Cron, String> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields.as_slice() else {
            return Err(format!("`{line}` has {} fields; a schedule has five: minute hour day month weekday", fields.len()));
        };
        let mut weekday = field(weekday, 5, 0, 7, &DAY_NAMES, 0)?;
        // 7 is Sunday too
        if weekday.has(7) {
            weekday.bits |= 1;
        }
        Ok(Cron {
            minute: field(minute, 1, 0, 59, &[], 0)?,
            hour: field(hour, 2, 0, 23, &[], 0)?,
            day: field(day, 3, 1, 31, &[], 0)?,
            month: field(month, 4, 1, 12, &MONTH_NAMES, 1)?,
            weekday,
        })
    }

    /// As in classic cron: when both day fields are limited, a day matching either is a match
    fn matches_day(&self, date: NaiveDate) -> bool {
        if !self.month.has(date.month()) {
            return false;
        }
        let by_day = self.day.has(date.day());
        let by_weekday = self.weekday.has(date.weekday().num_days_from_sunday());
        match (self.day.star, self.weekday.star) {
            (true, true) => true,
            (true, false) => by_weekday,
            (false, true) => by_day,
            (false, false) => by_day || by_weekday,
        }
    }

    /// The first time after `after` (not equal) that the line names, on the clock of `zone`. A
    /// clock time that does not exist (a daylight-saving gap) is skipped; one that happens twice
    /// counts once, at the earlier instant.
    pub fn next_after(&self, after: DateTime<Utc>, zone: Zone) -> Option<DateTime<Utc>> {
        let start = zone.naive(&after.fixed_offset()).with_second(0)?.with_nanosecond(0)? + Duration::minutes(1);
        let mut date = start.date();
        for _ in 0..LOOK_AHEAD_DAYS {
            if self.matches_day(date) {
                for hour in (0..24).filter(|h| self.hour.has(*h)) {
                    for minute in (0..60).filter(|m| self.minute.has(*m)) {
                        let naive = date.and_time(NaiveTime::from_hms_opt(hour, minute, 0)?);
                        if naive < start {
                            continue;
                        }
                        if let Some(first) = zone.instants(naive).into_iter().next() {
                            if first > after {
                                return Some(first);
                            }
                        }
                    }
                }
            }
            date = date.succ_opt()?;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    fn next(line: &str, after: &str, zone: &str) -> String {
        let zone = Zone::parse(Some(zone)).unwrap();
        let time = Cron::parse(line).unwrap().next_after(at(after), zone).unwrap();
        let offset = zone.from_naive(zone.naive(&time.fixed_offset())).unwrap();
        offset.to_rfc3339()
    }

    #[test]
    fn fields_and_names() {
        assert_eq!(next("30 8 * * 1-5", "2026-11-13T09:00:00+05:30", "Asia/Kolkata"), "2026-11-16T08:30:00+05:30");
        assert_eq!(next("0 18 * * fri", "2026-11-10T12:00:00+05:30", "Asia/Kolkata"), "2026-11-13T18:00:00+05:30");
        assert_eq!(next("*/15 * * * *", "2026-11-10T10:07:00Z", "UTC"), "2026-11-10T10:15:00+00:00");
        assert_eq!(next("0 9 1 jan *", "2026-11-10T00:00:00Z", "UTC"), "2027-01-01T09:00:00+00:00");
        assert_eq!(next("0 0 * * 7", "2026-11-10T00:00:00Z", "UTC"), "2026-11-15T00:00:00+00:00");
        // Both day fields limited: either matches
        assert_eq!(next("0 0 13 * 5", "2026-11-10T00:00:00Z", "UTC"), "2026-11-13T00:00:00+00:00");
        // Exactly at a time: the next one, not the same
        assert_eq!(next("0 * * * *", "2026-11-10T10:00:00Z", "UTC"), "2026-11-10T11:00:00+00:00");
        assert_eq!(next("5/20 * * * *", "2026-11-10T10:06:00Z", "UTC"), "2026-11-10T10:25:00+00:00");
    }

    #[test]
    fn daylight_saving() {
        // Berlin, 2026-03-29: 02:00 → 03:00. 02:30 does not exist that day
        assert_eq!(next("30 2 * * *", "2026-03-29T00:00:00+01:00", "Europe/Berlin"), "2026-03-30T02:30:00+02:00");
        // 2026-10-25: 03:00 → 02:00. 02:30 happens twice; it runs once
        let zone = Zone::parse(Some("Europe/Berlin")).unwrap();
        let cron = Cron::parse("30 2 * * *").unwrap();
        let first = cron.next_after(at("2026-10-25T00:00:00+02:00"), zone).unwrap();
        assert_eq!(first, at("2026-10-25T02:30:00+02:00"));
        let second = cron.next_after(first, zone).unwrap();
        assert_eq!(second, at("2026-10-26T02:30:00+01:00"));
    }

    #[test]
    fn mistakes() {
        assert!(Cron::parse("0 9 * *").unwrap_err().contains("five"));
        assert!(Cron::parse("61 * * * *").is_err());
        assert!(Cron::parse("*/0 * * * *").is_err());
        assert!(Cron::parse("0 0 * * funday").is_err());
        assert!(Cron::parse("0 5-2 * * *").is_err());
        assert!(Cron::parse("0 0 31 2 *").unwrap().next_after(at("2026-01-01T00:00:00Z"), Zone::Utc).is_none());
    }
}
