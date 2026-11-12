//! `schedule` and `every`: wait for the next planned time, start a run, and run missed times
//! as `catch_up` says (after Automation was off, the flow disabled, or the computer asleep).

use super::{Fired, Place};
use chrono::{DateTime, Duration, Utc};
use cluaiz_automation::trigger::timing::{missed, Clock};
use cluaiz_automation::trigger::{state, time_text, Owner, Spec};
use cluaiz_automation::units::parse_duration;
use serde_json::{json, Value};

/// Sleeps are cut into pieces this long and the wall clock is read again, so a computer that
/// slept does not delay a time by the length of its sleep
const NAP_MS: i64 = 30_000;

pub async fn wait_until(time: DateTime<Utc>) {
    loop {
        let left = (time - Utc::now()).num_milliseconds();
        if left <= 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(left.min(NAP_MS) as u64)).await;
    }
}

fn random_ms(up_to: i64) -> i64 {
    if up_to <= 0 {
        return 0;
    }
    (uuid::Uuid::new_v4().as_u128() % (up_to as u128 + 1)) as i64
}

async fn start(at: &Place, planned: DateTime<Utc>, caught_up: bool) {
    let data = json!({ "scheduled_for": time_text(planned), "caught_up": caught_up });
    if let Fired::Skipped(message) = at.fire(data).await {
        tracing::info!("Automation: {} of {} skipped {}: {message}", at.id, at.name, time_text(planned));
    }
}

pub async fn run(at: Place, spec: Spec, flow: Value) {
    let paths = at.service.paths.clone();
    let owner = Owner { paths: &paths, scope: at.scope.as_str(), flow: &flow };
    let now = Utc::now();
    // `every` without `starting` counts from the first time the trigger listened
    let since = match state::of(&owner, &at.id).get("since").and_then(Value::as_str).and_then(|t| DateTime::parse_from_rfc3339(t).ok()) {
        Some(since) => since.with_timezone(&Utc),
        None => {
            state::update(&owner, &at.id, |s| {
                s.insert("since".into(), json!(time_text(now)));
            });
            now
        }
    };
    let clock = match Clock::of(&flow, &spec, since) {
        Ok(clock) => clock,
        Err(message) => return at.set(|l| *l = super::Live::error(message)),
    };
    let catch_up = spec.beside("catch_up").and_then(Value::as_str).unwrap_or("skip").to_string();
    let jitter_ms = spec.beside("jitter").and_then(Value::as_str).and_then(|j| parse_duration(j).ok()).unwrap_or(0);
    let mut after = now;
    if let Some(last) = state::last_planned(&owner, &at.id) {
        for planned in missed(&clock, last, now, &catch_up) {
            start(&at, planned, true).await;
        }
    }
    state::set_last_planned(&owner, &at.id, now);
    loop {
        let Some(next) = clock.next_after(after) else {
            return at.set(|l| *l = super::Live::error("the schedule names no time in the next eight years"));
        };
        at.set(|l| l.next_at = Some(time_text(next)));
        wait_until(next + Duration::milliseconds(random_ms(jitter_ms))).await;
        start(&at, next, false).await;
        state::set_last_planned(&owner, &at.id, next);
        // Times passed while the computer slept follow catch_up too
        let now = Utc::now();
        for planned in missed(&clock, next, now, &catch_up) {
            start(&at, planned, true).await;
        }
        after = now.max(next);
        state::set_last_planned(&owner, &at.id, after);
    }
}
