//! `poll`: every N minutes run the tool or method as the flow, and start a run when the answer
//! has something new. A failing poll is recorded on the trigger and tried again next time.

use super::clock::wait_until;
use super::{Live, Place};
use crate::engine::EngineDoor;
use chrono::{Duration, Utc};
use cluaiz_automation::expr::values::Zone;
use cluaiz_automation::host::RunInfo;
use cluaiz_automation::trigger::poll::{compare, poll_once};
use cluaiz_automation::trigger::{state, time_text, Owner, Spec};
use cluaiz_automation::units::parse_duration;
use serde_json::{json, Value};

pub async fn run(at: Place, spec: Spec, flow: Value, permissions: Value) {
    let paths = at.service.paths.clone();
    let owner = Owner { paths: &paths, scope: at.scope.as_str(), flow: &flow };
    let every = spec.text("every").and_then(|e| parse_duration(e).ok()).unwrap_or(cluaiz_automation::trigger::MIN_INTERVAL_MS).max(cluaiz_automation::trigger::MIN_INTERVAL_MS);
    let zone = Zone::parse(flow.get("timezone").and_then(Value::as_str)).unwrap_or(Zone::Local);
    let run = RunInfo { flow: at.name.clone(), scope: at.scope.as_str().to_string(), run_id: format!("poll_{}", at.id), permissions, flow_dir: None };
    loop {
        let started = Utc::now();
        let door = EngineDoor { state: at.service.state.clone(), catalog: at.service.snapshot().await };
        match poll_once(&door, &run, &spec).await {
            Err(error) => {
                let message = format!("{}: {}", error.kind, error.message);
                state::failed(&owner, &at.id, &message);
                at.set(|l| *l = Live { status: "error", next_at: None, message: Some(message) });
            }
            Ok(output) => {
                let kept = state::of(&owner, &at.id).get("poll").cloned().unwrap_or_else(|| json!({}));
                match compare(&spec, &output, &kept, &paths, zone) {
                    Err(error) => {
                        state::failed(&owner, &at.id, &error.message);
                        at.set(|l| *l = Live::error(error.message.clone()));
                    }
                    Ok((data, keep)) => {
                        state::update(&owner, &at.id, |s| {
                            s.insert("poll".into(), keep);
                        });
                        state::worked(&owner, &at.id);
                        at.set(|l| *l = Live::new("active"));
                        if let Some(data) = data {
                            at.fire(data).await;
                        }
                    }
                }
            }
        }
        let next = started + Duration::milliseconds(every);
        at.set(|l| l.next_at = Some(time_text(next)));
        wait_until(next).await;
    }
}
