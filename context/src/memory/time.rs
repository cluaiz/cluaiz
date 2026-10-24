//! Time questions ("kal kya baat hui?", "Monday ko", "23 Oct", "subah", "3 ghante pehle"): the
//! window of time a message names, in the user's own days (their clock's offset), so recall can
//! keep to the messages of that time. Words come from `data/words.json` (`memory.time`).

use crate::analyze::raw_words;
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, Offset, TimeZone, Timelike, Utc};
use serde_json::Value;
use std::sync::LazyLock;

/// A time window named in a message
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

static TIME_WORDS: LazyLock<Value> = LazyLock::new(|| {
    let data: Value = serde_json::from_str(include_str!("../../data/words.json")).unwrap_or_default();
    data.pointer("/memory/time").cloned().unwrap_or_default()
});

fn phrases(path: &str) -> Vec<Vec<String>> {
    TIME_WORDS
        .pointer(path)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(raw_words).collect())
        .unwrap_or_default()
}

fn has(words: &[String], path: &str) -> bool {
    phrases(path).iter().any(|p| !p.is_empty() && words.windows(p.len()).any(|w| w == p.as_slice()))
}

/// The number written just before one of the phrases at `path` ("2 din pehle" -> 2)
fn number_before(words: &[String], path: &str) -> Option<i64> {
    phrases(path).iter().find_map(|p| {
        let at = words.windows(p.len()).position(|w| w == p.as_slice())?;
        words.get(at.checked_sub(1)?)?.parse::<i64>().ok()
    })
}

/// Month number of a month word ("oct", "october", "अक्टूबर")
fn month_of(word: &str) -> Option<u32> {
    let months = TIME_WORDS.pointer("/months").and_then(Value::as_array)?;
    months.iter().position(|names| {
        names.as_array().is_some_and(|names| names.iter().filter_map(Value::as_str).any(|n| n == word))
    }).map(|i| i as u32 + 1)
}

/// A date written in the message: 2026-10-23, 23 Oct, Oct 23, 23 October. A day and month
/// without a year is the latest one not after today.
fn written_date(words: &[String], today: NaiveDate) -> Option<NaiveDate> {
    let num = |w: &String| w.parse::<u32>().ok();
    for w in words.windows(3) {
        if let (Some(y), Some(m), Some(d)) = (num(&w[0]), num(&w[1]), num(&w[2])) {
            if (1970..=2200).contains(&y) {
                if let Some(date) = NaiveDate::from_ymd_opt(y as i32, m, d) {
                    return Some(date);
                }
            }
        }
    }
    for w in words.windows(2) {
        let (day, month) = match (num(&w[0]), month_of(&w[1]), month_of(&w[0]), num(&w[1])) {
            (Some(d), Some(m), _, _) => (d, m),
            (_, _, Some(m), Some(d)) => (d, m),
            _ => continue,
        };
        let this_year = NaiveDate::from_ymd_opt(today.year(), month, day)?;
        return Some(if this_year > today { NaiveDate::from_ymd_opt(today.year() - 1, month, day)? } else { this_year });
    }
    None
}

/// The window a message names, if any, with days as the user's clock (`offset`) counts them
pub fn time_window_in(text: &str, now: DateTime<Utc>, offset: FixedOffset) -> Option<Window> {
    let words = raw_words(text);
    let local_now = now.with_timezone(&offset);
    let today = local_now.date_naive();
    let at = |date: NaiveDate, hour: u32| -> DateTime<Utc> {
        let time = NaiveTime::from_hms_opt(hour.min(23), 0, 0).unwrap_or(NaiveTime::MIN);
        let local = offset.from_local_datetime(&date.and_time(time)).single().unwrap_or_else(|| offset.from_utc_datetime(&date.and_time(time)));
        local.with_timezone(&Utc) + Duration::hours(hour.saturating_sub(23) as i64)
    };
    let day = |date: NaiveDate| Window { from: at(date, 0), to: at(date + Duration::days(1), 0) };

    // Hours: "3 ghante pehle", "thodi der pehle"
    if let Some(n) = number_before(&words, "/hours_ago") {
        return Some(Window { from: now - Duration::hours(n + 1), to: now });
    }
    if has(&words, "/recent") {
        return Some(Window { from: now - Duration::hours(2), to: now });
    }

    // The day (or longer span) it names
    let named_day = if let Some(n) = number_before(&words, "/days_ago") {
        Some(today - Duration::days(n))
    } else if let Some(date) = written_date(&words, today) {
        Some(date)
    } else if has(&words, "/day_before_yesterday") {
        Some(today - Duration::days(2))
    } else if has(&words, "/yesterday") {
        Some(today - Duration::days(1))
    } else if has(&words, "/today") {
        Some(today)
    } else {
        // A weekday: its latest one, today included
        (0..7u32).find(|i| has(&words, &format!("/weekdays/{i}"))).map(|weekday| {
            let back = (today.weekday().num_days_from_monday() + 7 - weekday) % 7;
            today - Duration::days(back as i64)
        })
    };

    // A part of the day narrows a day ("kal subah"); alone it means the latest one
    let part = (0..4usize).find(|i| has(&words, &format!("/parts/{i}/words")));
    if let Some(i) = part {
        let hours = |key: &str| TIME_WORDS.pointer(&format!("/parts/{i}/{key}")).and_then(Value::as_u64).unwrap_or(0) as u32;
        let (start, end) = (hours("from"), hours("to"));
        let date = named_day.unwrap_or_else(|| if local_now.hour() >= start { today } else { today - Duration::days(1) });
        return Some(Window { from: at(date, start), to: at(date, end) });
    }
    if let Some(date) = named_day {
        return Some(day(date));
    }
    if has(&words, "/week") {
        return Some(Window { from: day(today - Duration::days(7)).from, to: now });
    }
    if has(&words, "/month") {
        return Some(Window { from: day(today - Duration::days(31)).from, to: now });
    }
    None
}

/// The window a message names, with days as this machine's clock counts them
pub fn time_window(text: &str, now: DateTime<Utc>) -> Option<Window> {
    time_window_in(text, now, chrono::Local::now().offset().fix())
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
    fn w(text: &str) -> Option<(String, String)> {
        time_window_in(text, now(), ist()).map(|w| (w.from.with_timezone(&ist()).to_rfc3339(), w.to.with_timezone(&ist()).to_rfc3339()))
    }

    #[test]
    fn test_days_in_the_users_clock() {
        assert_eq!(w("kal kya baat hui?").unwrap(), ("2026-10-23T00:00:00+05:30".into(), "2026-10-24T00:00:00+05:30".into()));
        assert_eq!(w("aaj subah kya kiya").unwrap(), ("2026-10-24T05:00:00+05:30".into(), "2026-10-24T12:00:00+05:30".into()));
        assert_eq!(w("parso wala bug").unwrap().0, "2026-10-22T00:00:00+05:30");
        assert_eq!(w("2 din pehle maine kya kaha").unwrap().0, "2026-10-22T00:00:00+05:30");
        assert_eq!(w("कल क्या हुआ").unwrap().0, "2026-10-23T00:00:00+05:30");
        assert!(w("what is rust").is_none());
    }

    #[test]
    fn test_weekdays_dates_and_hours() {
        // Monday before Saturday the 24th is the 19th; Saturday is today
        assert_eq!(w("Monday ko kya decide hua?").unwrap().0, "2026-10-19T00:00:00+05:30");
        assert_eq!(w("on saturday").unwrap().0, "2026-10-24T00:00:00+05:30");
        assert_eq!(w("what did we do on 23 Oct").unwrap().0, "2026-10-23T00:00:00+05:30");
        assert_eq!(w("oct 25 meeting").unwrap().0, "2025-10-25T00:00:00+05:30");
        assert_eq!(w("2026-10-20 ka log").unwrap().0, "2026-10-20T00:00:00+05:30");
        assert_eq!(w("kal raat").unwrap(), ("2026-10-23T21:00:00+05:30".into(), "2026-10-24T05:00:00+05:30".into()));
        assert_eq!(w("3 ghante pehle").unwrap().0, "2026-10-24T13:30:00+05:30");
        assert_eq!(w("abhi thodi der pehle kya bola").unwrap().0, "2026-10-24T15:30:00+05:30");
        assert!(w("last week").is_some() && w("pichhle mahine").is_some());
    }
}
