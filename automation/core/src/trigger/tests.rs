use super::*;
use chrono::Utc;

fn flow() -> Value {
    json!({
        "flow": "cluaiz.flow/1",
        "name": "orders",
        "on": [
            { "webhook": { "secret": "env:H" }, "filter": { "eq": [{ "var": "trigger.body.type" }, "order"] },
              "dedupe": "${{ trigger.body.id }}", "inputs": { "order": "${{ trigger.body.id }}" } },
            { "id": "nightly", "schedule": "0 2 * * *", "enabled": false },
            { "manual": {} }
        ],
        "steps": [{ "set": { "a": 1 } }]
    })
}

#[test]
fn ids_kinds_and_switches() {
    let all = specs(&flow());
    let ids: Vec<(&str, &str, bool)> = all.iter().map(|s| (s.id.as_str(), s.kind, s.enabled())).collect();
    assert_eq!(ids, vec![("webhook_0", "webhook", true), ("nightly", "schedule", false), ("manual_2", "manual", true)]);
    assert_eq!(all[1].body(), &json!("0 2 * * *"));
    assert_eq!(all[0].text("secret"), Some("env:H"));
    assert!(all.iter().all(Spec::built));
}

#[test]
fn two_events_with_one_dedupe_value_give_one_run() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths { flows: dir.path().to_path_buf() };
    let flow = flow();
    let owner = Owner { paths: &paths, scope: "user", flow: &flow };
    let spec = specs(&flow).remove(0);
    let now = Utc::now();
    let event = json!({ "body": { "type": "order", "id": "o-1" } });
    match admit(&owner, &spec, event.clone(), now).unwrap() {
        Admit::Run { trigger, inputs } => {
            assert_eq!(inputs, json!({ "order": "o-1" }));
            assert_eq!((trigger["kind"].clone(), trigger["id"].clone()), (json!("webhook"), json!("webhook_0")));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(admit(&owner, &spec, event, now).unwrap(), Admit::Duplicate);
    assert!(matches!(admit(&owner, &spec, json!({ "body": { "type": "order", "id": "o-2" } }), now).unwrap(), Admit::Run { .. }));
    assert_eq!(admit(&owner, &spec, json!({ "body": { "type": "refund", "id": "o-3" } }), now).unwrap(), Admit::Filtered);
    // A value is forgotten after dedupe_for (24h by default)
    let later = now + chrono::Duration::hours(25);
    assert!(matches!(admit(&owner, &spec, json!({ "body": { "type": "order", "id": "o-1" } }), later).unwrap(), Admit::Run { .. }));
}

#[test]
fn kept_state_counts_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths { flows: dir.path().to_path_buf() };
    let flow = flow();
    let owner = Owner { paths: &paths, scope: "user", flow: &flow };
    state::count(&owner, "webhook_0", state::Count::Runs);
    state::count(&owner, "webhook_0", state::Count::Runs);
    state::failed(&owner, "webhook_0", "the tool did not answer");
    let kept = state::of(&owner, "webhook_0");
    assert_eq!(kept["counts"], json!({ "runs": 2, "errors": 1 }));
    assert_eq!(kept["last_error"]["message"], "the tool did not answer");
    state::worked(&owner, "webhook_0");
    assert!(state::of(&owner, "webhook_0").get("last_error").is_none());
    let at = Utc::now();
    state::set_last_planned(&owner, "nightly", at);
    assert_eq!(state::last_planned(&owner, "nightly").map(|t| t.timestamp_millis()), Some(at.timestamp_millis()));
}

#[test]
fn a_filter_that_cannot_be_worked_out_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths { flows: dir.path().to_path_buf() };
    let flow = json!({ "name": "f", "on": [{ "manual": {}, "filter": { "var": "trigger.by" } }] });
    let owner = Owner { paths: &paths, scope: "user", flow: &flow };
    let spec = specs(&flow).remove(0);
    assert!(admit(&owner, &spec, json!({ "by": "cli" }), Utc::now()).is_err());
}
