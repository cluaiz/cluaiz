//! Time operators. Times are RFC 3339 text; day-of-week and time-of-day use the flow's timezone.

use super::values::{format_time, number, parse_time};
use super::Env;
use crate::units::parse_duration;
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc};
use serde_json::Value;

type Out = Result<Value, String>;

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn time(value: &Value) -> Result<Option<DateTime<FixedOffset>>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => parse_time(text).map(Some).ok_or_else(|| format!("\"{text}\" is not an RFC 3339 time")),
        other => Err(format!("needs a time as RFC 3339 text, not {other}")),
    }
}

fn duration(value: &Value) -> Result<Duration, String> {
    let text = value.as_str().ok_or("needs a duration such as 2h or 7d")?;
    Ok(Duration::milliseconds(parse_duration(text)?))
}

fn clock(text: &Value) -> Result<NaiveTime, String> {
    let text = text.as_str().ok_or("needs a time of day such as \"09:00\"")?;
    NaiveTime::parse_from_str(text, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(text, "%H:%M:%S"))
        .map_err(|_| format!("\"{text}\" is not a time of day such as \"09:00\""))
}

pub fn apply(env: &Env, name: &str, a: &[Value]) -> Out {
    let zone = env.zone;
    match name {
        "before" | "after" => match (time(&a[0])?, time(&a[1])?) {
            (Some(t1), Some(t2)) => Ok(Value::Bool(if name == "before" { t1 < t2 } else { t1 > t2 })),
            _ => Ok(Value::Bool(false)),
        },
        "within" => {
            let Some(t) = time(&a[0])? else { return Ok(Value::Bool(false)) };
            let span = duration(&a[1])?;
            let now = env.now.fixed_offset();
            Ok(Value::Bool(t <= now && t >= now - span))
        }
        "weekday" => {
            let Some(t) = time(&a[0])? else { return Ok(Value::Bool(false)) };
            let day = DAYS[zone.naive(&t).weekday().num_days_from_monday() as usize];
            let wanted = match &a[1] {
                Value::String(one) => vec![one.as_str()],
                Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
                other => return Err(format!("weekday needs day names such as [\"sat\", \"sun\"], not {other}")),
            };
            if let Some(bad) = wanted.iter().find(|d| !DAYS.contains(d)) {
                return Err(format!("\"{bad}\" is not a day: use mon, tue, wed, thu, fri, sat or sun"));
            }
            Ok(Value::Bool(wanted.contains(&day)))
        }
        "time_between" => {
            let Some(t) = time(&a[0])? else { return Ok(Value::Bool(false)) };
            let at = zone.naive(&t).time();
            let (from, to) = (clock(&a[1])?, clock(&a[2])?);
            Ok(Value::Bool(if from <= to { at >= from && at <= to } else { at >= from || at <= to }))
        }
        "add_time" => {
            let Some(t) = time(&a[0])? else { return Ok(Value::Null) };
            Ok(Value::String(format_time(&(t + duration(&a[1])?))))
        }
        "diff_time" => {
            let (Some(t1), Some(t2)) = (time(&a[0])?, time(&a[1])?) else { return Ok(Value::Null) };
            let ms = (t1 - t2).num_milliseconds() as f64;
            let unit = a[2].as_str().ok_or("diff_time needs a unit: ms, s, m, h or d")?;
            let per = match unit {
                "ms" => 1.0,
                "s" => 1_000.0,
                "m" => 60_000.0,
                "h" => 3_600_000.0,
                "d" => 86_400_000.0,
                other => return Err(format!("\"{other}\" is not a unit: use ms, s, m, h or d")),
            };
            number(ms / per)
        }
        "start_of" => {
            let Some(t) = time(&a[0])? else { return Ok(Value::Null) };
            let local = zone.naive(&t).date();
            let date = match a[1].as_str().unwrap_or_default() {
                "day" => local,
                "week" => local - Duration::days(local.weekday().num_days_from_monday() as i64),
                "month" => local.with_day(1).unwrap_or(local),
                "year" => NaiveDate::from_ymd_opt(local.year(), 1, 1).unwrap_or(local),
                other => return Err(format!("\"{other}\" is not day, week, month or year")),
            };
            let start = zone.from_naive(date.and_time(NaiveTime::MIN)).ok_or("that time does not exist in the timezone")?;
            Ok(Value::String(format_time(&start)))
        }
        "format_time" => {
            let Some(t) = time(&a[0])? else { return Ok(Value::Null) };
            let format = a[1].as_str().ok_or("format_time needs a format such as \"DD MMM YYYY\"")?;
            Ok(Value::String(format_with(&zone.naive(&t), format)))
        }
        "parse_time" => {
            let text = a[0].as_str().ok_or("parse_time needs text")?;
            let format = a[1].as_str().ok_or("parse_time needs a format such as \"DD/MM/YYYY\"")?;
            let naive = parse_with(text, format)?;
            let parsed = zone.from_naive(naive).ok_or("that time does not exist in the timezone")?;
            Ok(Value::String(format_time(&parsed)))
        }
        "to_time" => match &a[0] {
            Value::Null => Ok(Value::Null),
            Value::Number(ms) => {
                let ms = ms.as_i64().ok_or("to_time needs whole milliseconds")?;
                let at = Utc.timestamp_millis_opt(ms).single().ok_or("the number is outside the times that can be shown")?;
                Ok(Value::String(format_time(&at.fixed_offset())))
            }
            other => time(other).map(|t| t.map(|t| Value::String(format_time(&t))).unwrap_or(Value::Null)),
        },
        "to_unix_ms" => Ok(time(&a[0])?.map(|t| Value::from(t.timestamp_millis())).unwrap_or(Value::Null)),
        other => Err(format!("`{other}` is not built")),
    }
}

/// Tokens: YYYY MM DD HH mm ss ddd MMM; other characters are kept
fn format_with(t: &NaiveDateTime, format: &str) -> String {
    let mut out = String::new();
    let mut rest = format;
    while !rest.is_empty() {
        let (token, len) = match rest {
            r if r.starts_with("YYYY") => (format!("{:04}", t.year()), 4),
            r if r.starts_with("MMM") => (MONTHS[t.month0() as usize].to_string(), 3),
            r if r.starts_with("MM") => (format!("{:02}", t.month()), 2),
            r if r.starts_with("DD") => (format!("{:02}", t.day()), 2),
            r if r.starts_with("ddd") => (capitalised(DAYS[t.weekday().num_days_from_monday() as usize]), 3),
            r if r.starts_with("HH") => (format!("{:02}", t.hour()), 2),
            r if r.starts_with("mm") => (format!("{:02}", t.minute()), 2),
            r if r.starts_with("ss") => (format!("{:02}", t.second()), 2),
            r => {
                let c = r.chars().next().unwrap_or_default();
                (c.to_string(), c.len_utf8())
            }
        };
        out.push_str(&token);
        rest = &rest[len..];
    }
    out
}

fn capitalised(day: &str) -> String {
    let mut chars = day.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

fn parse_with(text: &str, format: &str) -> Result<NaiveDateTime, String> {
    let bad = || format!("\"{text}\" does not fit the format \"{format}\"");
    let (mut year, mut month, mut day, mut hour, mut minute, mut second) = (None, None, None, 0u32, 0u32, 0u32);
    let mut f = format;
    let mut t = text;
    let digits = |t: &mut &str, n: usize| -> Result<u32, String> {
        let part = t.get(..n).filter(|p| p.chars().all(|c| c.is_ascii_digit())).ok_or_else(bad)?;
        *t = &t[n..];
        part.parse().map_err(|_| bad())
    };
    while !f.is_empty() {
        if f.starts_with("YYYY") {
            year = Some(digits(&mut t, 4)? as i32);
            f = &f[4..];
        } else if f.starts_with("MMM") {
            let name = t.get(..3).ok_or_else(bad)?;
            month = Some(MONTHS.iter().position(|m| m.eq_ignore_ascii_case(name)).ok_or_else(bad)? as u32 + 1);
            t = &t[3..];
            f = &f[3..];
        } else if f.starts_with("MM") {
            month = Some(digits(&mut t, 2)?);
            f = &f[2..];
        } else if f.starts_with("DD") {
            day = Some(digits(&mut t, 2)?);
            f = &f[2..];
        } else if f.starts_with("HH") {
            hour = digits(&mut t, 2)?;
            f = &f[2..];
        } else if f.starts_with("mm") {
            minute = digits(&mut t, 2)?;
            f = &f[2..];
        } else if f.starts_with("ss") {
            second = digits(&mut t, 2)?;
            f = &f[2..];
        } else {
            let c = f.chars().next().unwrap_or_default();
            if !t.starts_with(c) {
                return Err(bad());
            }
            f = &f[c.len_utf8()..];
            t = &t[c.len_utf8()..];
        }
    }
    if !t.is_empty() {
        return Err(bad());
    }
    let date = NaiveDate::from_ymd_opt(year.ok_or_else(bad)?, month.ok_or_else(bad)?, day.unwrap_or(1)).ok_or_else(bad)?;
    let clock = NaiveTime::from_hms_opt(hour, minute, second).ok_or_else(bad)?;
    Ok(date.and_time(clock))
}
