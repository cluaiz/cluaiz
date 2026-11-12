//! The examples of docs/automation/expressions.md, each checked.

use super::values::Zone;
use super::*;
use chrono::TimeZone;
use serde_json::json;

fn env_with(root: &Value) -> Env<'_> {
    Env { root, zone: Zone::parse(Some("Asia/Kolkata")).unwrap(), now: Utc.with_ymd_and_hms(2026, 11, 10, 12, 0, 0).unwrap(), previous: None }
}

fn run(expr: Value) -> Result<Value, ExprError> {
    let root = json!({
        "inputs": { "days": 7, "name": "asha" },
        "steps": { "order": { "output": { "total": 6200, "city": "Delhi" } } },
        "trigger": { "body": { "items": [{ "n": "a" }, { "n": "b" }] } }
    });
    let env = env_with(&root);
    Evaluator::new(&env).eval(&expr)
}

fn ok(expr: Value) -> Value {
    run(expr.clone()).unwrap_or_else(|e| panic!("{expr}: {e}"))
}

#[test]
fn the_example_at_the_top() {
    let expr = json!({ "all": [
        { "gt": [{ "var": "steps.order.output.total" }, 5000] },
        { "in": [{ "lower": { "var": "steps.order.output.city" } }, ["delhi", "mumbai"]] }
    ] });
    assert_eq!(ok(expr), json!(true));
}

#[test]
fn values_and_logic() {
    assert_eq!(ok(json!({ "var": "inputs.days" })), json!(7));
    assert_eq!(ok(json!({ "var": ["inputs.missing", 7] })), json!(7));
    assert_eq!(ok(json!({ "literal": { "gt": 1 } })), json!({ "gt": 1 }));
    assert_eq!(ok(json!({ "default": [null, "", "x"] })), json!(""));
    assert_eq!(ok(json!({ "choose": [true, "a", "b"] })), json!("a"));
    assert_eq!(ok(json!({ "all": [] })), json!(true));
    assert_eq!(ok(json!({ "any": [] })), json!(false));
    assert!(run(json!({ "not": 1 })).is_err());
    // short circuit: the second argument would be an error
    assert_eq!(ok(json!({ "any": [true, { "not": 1 }] })), json!(true));
}

#[test]
fn comparisons() {
    assert_eq!(ok(json!({ "eq": ["Delhi", "delhi"] })), json!(false));
    assert_eq!(ok(json!({ "eq": [{ "lower": "Delhi" }, "delhi"] })), json!(true));
    assert_eq!(ok(json!({ "gt": [{ "var": "missing" }, 5] })), json!(false));
    let error = run(json!({ "gt": ["10", 5] })).unwrap_err();
    assert!(error.to_string().contains("cannot compare text with a number"), "{error}");
    assert_eq!(ok(json!({ "lt": ["2026-11-01T00:00:00Z", { "var": "now" }] })), json!(true));
    assert_eq!(ok(json!({ "between": [5, 1, 5] })), json!(true));
}

#[test]
fn text() {
    assert_eq!(ok(json!({ "contains": ["invoice 42", "42"] })), json!(true));
    assert_eq!(ok(json!({ "matches": ["Order #123", "#\\d+"] })), json!(true));
    assert_eq!(ok(json!({ "find": ["id=77", "id=(\\d+)"] })), json!({ "match": "id=77", "groups": ["77"] }));
    assert_eq!(ok(json!({ "upper": "abc" })), json!("ABC"));
    assert_eq!(ok(json!({ "trim": "  hi \n" })), json!("hi"));
    assert_eq!(ok(json!({ "replace": ["a-b-c", "-", " "] })), json!("a b c"));
    assert_eq!(ok(json!({ "replace_pattern": ["2026-11-10", "(\\d+)-(\\d+)-(\\d+)", "$3/$2/$1"] })), json!("10/11/2026"));
    assert_eq!(ok(json!({ "split": ["a,b,c", ","] })), json!(["a", "b", "c"]));
    assert_eq!(ok(json!({ "join": [["a", "b"], ", "] })), json!("a, b"));
    assert_eq!(ok(json!({ "concat": ["#", 42] })), json!("#42"));
    assert_eq!(ok(json!({ "substring": ["abcdef", 1, 3] })), json!("bcd"));
    assert_eq!(ok(json!({ "size": "नमस्ते" })), json!(6));
    assert!(run(json!({ "upper": 5 })).is_err());
}

#[test]
fn lists() {
    assert_eq!(ok(json!({ "in": ["b", ["a", "b"]] })), json!(true));
    assert_eq!(ok(json!({ "size": [1, 2, 3] })), json!(3));
    assert_eq!(ok(json!({ "empty": [] })), json!(true));
    assert_eq!(ok(json!({ "some": [[1, 5], { "gt": [{ "var": "item" }, 3] }] })), json!(true));
    assert_eq!(ok(json!({ "filter": [[1, 5, 7], { "gt": [{ "var": "item" }, 3] }] })), json!([5, 7]));
    assert_eq!(ok(json!({ "map": [[1, 2], { "mul": [{ "var": "item" }, 10] }] })), json!([10, 20]));
    assert_eq!(ok(json!({ "pluck": [[{ "n": "a" }, { "n": "b" }], "n"] })), json!(["a", "b"]));
    assert_eq!(ok(json!({ "last": [1, 2, 3] })), json!(3));
    assert_eq!(ok(json!({ "slice": [[1, 2, 3, 4], 1, 3] })), json!([2, 3]));
    assert_eq!(ok(json!({ "sort": [[3, 1, 2]] })), json!([1, 2, 3]));
    assert_eq!(ok(json!({ "unique": [1, 1, 2] })), json!([1, 2]));
    assert_eq!(ok(json!({ "flatten": [[1, [2]], [3]] })), json!([1, [2], 3]));
    assert_eq!(ok(json!({ "sum": [1, 2, 3] })), json!(6));
    assert_eq!(ok(json!({ "avg": [] })), Value::Null);
    assert_eq!(ok(json!({ "range": [0, 3] })), json!([0, 1, 2]));
    assert_eq!(ok(json!({ "count": [{ "var": "trigger.body.items" }, { "eq": [{ "var": "item.n" }, "b"] }] })), json!(1));
    assert_eq!(ok(json!({ "sort": [[{ "p": 2 }, { "p": 1 }], "p", "desc"] })), json!([{ "p": 2 }, { "p": 1 }]));
}

#[test]
fn objects_and_types() {
    assert_eq!(ok(json!({ "keys": { "a": 1, "b": 2 } })), json!(["a", "b"]));
    assert_eq!(ok(json!({ "merge": [{ "a": 1 }, { "a": 2, "b": 3 }] })), json!({ "a": 2, "b": 3 }));
    assert_eq!(ok(json!({ "object": { "total": { "sum": [1, 2] } } })), json!({ "total": 3 }));
    assert_eq!(ok(json!({ "exists": "inputs.name" })), json!(true));
    assert_eq!(ok(json!({ "exists": "inputs.nothing" })), json!(false));
    assert_eq!(ok(json!({ "is": ["2026-11-10T18:30:00+05:30", "time"] })), json!(true));
    assert_eq!(ok(json!({ "to_number": "12.5" })), json!(12.5));
    assert_eq!(ok(json!({ "to_integer": "-3.9" })), json!(-3));
    assert_eq!(ok(json!({ "to_bool": "yes" })), json!(true));
    assert!(run(json!({ "to_bool": "maybe" })).is_err());
    assert_eq!(ok(json!({ "eq": ["5", 5] })), json!(false));
}

#[test]
fn numbers() {
    assert_eq!(ok(json!({ "add": [1, 2, 3] })), json!(6));
    assert_eq!(ok(json!({ "div": [7, 2] })), json!(3.5));
    assert_eq!(ok(json!({ "mod": [7, 3] })), json!(1));
    assert_eq!(ok(json!({ "round": [2.345, 2] })), json!(2.35));
    assert_eq!(ok(json!({ "max": [3, 9, 1] })), json!(9));
    assert!(run(json!({ "div": [1, 0] })).is_err());
}

#[test]
fn time() {
    // 2026-11-10 is a Tuesday; 12:00 UTC is 17:30 in Kolkata
    assert_eq!(ok(json!({ "weekday": [{ "var": "now" }, ["tue"]] })), json!(true));
    assert_eq!(ok(json!({ "time_between": [{ "var": "now" }, "17:00", "18:00"] })), json!(true));
    assert_eq!(ok(json!({ "time_between": [{ "var": "now" }, "22:00", "06:00"] })), json!(false));
    assert_eq!(ok(json!({ "add_time": ["2026-11-10T00:00:00Z", "-7d"] })), json!("2026-11-03T00:00:00Z"));
    assert_eq!(ok(json!({ "diff_time": ["2026-11-10T02:00:00Z", "2026-11-10T00:00:00Z", "h"] })), json!(2));
    assert_eq!(ok(json!({ "format_time": ["2026-11-10T00:00:00Z", "DD MMM YYYY"] })), json!("10 Nov 2026"));
    assert_eq!(ok(json!({ "parse_time": ["10/11/2026", "DD/MM/YYYY"] })), json!("2026-11-10T00:00:00+05:30"));
    assert_eq!(ok(json!({ "start_of": ["2026-11-10T12:00:00Z", "week"] })), json!("2026-11-09T00:00:00+05:30"));
    assert_eq!(ok(json!({ "to_unix_ms": "1970-01-01T00:00:01Z" })), json!(1000));
    assert_eq!(ok(json!({ "within": ["2026-11-10T11:00:00Z", "2h"] })), json!(true));
}

#[test]
fn encoding() {
    assert_eq!(ok(json!({ "base64_encode": "hi" })), json!("aGk="));
    assert_eq!(ok(json!({ "base64_decode": "aGk=" })), json!("hi"));
    assert_eq!(ok(json!({ "url_encode": "a b&c" })), json!("a%20b%26c"));
    assert_eq!(ok(json!({ "sha256": "" })), json!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
}

#[test]
fn changed_and_previous_read_the_last_succeeded_run() {
    let root = json!({ "inputs": { "price": 10 } });
    let previous: Map<String, Value> = [("inputs.price".to_string(), json!(9))].into_iter().collect();
    let env = Env { previous: Some(&previous), ..env_with(&root) };
    let mut ev = Evaluator::new(&env);
    assert_eq!(ev.eval(&json!({ "changed": "inputs.price" })).unwrap(), json!(true));
    assert_eq!(ev.eval(&json!({ "previous": "inputs.price" })).unwrap(), json!(9));
    assert_eq!(ev.watched[0], ("inputs.price".to_string(), json!(10)));
}

#[test]
fn decisions_need_booleans_and_errors_name_the_place() {
    let root = json!({});
    let env = env_with(&root);
    assert!(Evaluator::new(&env).decide(&json!(0)).is_err());
    let error = Evaluator::new(&env).eval(&json!({ "all": [true, { "gt": ["a", 1] }] })).unwrap_err();
    assert_eq!(error.path, "all[1].gt");
}

#[test]
fn shapes_are_checked_before_a_run() {
    let mut problems = Vec::new();
    check_shape(&json!({ "all": [{ "gtt": [1, 2] }, { "between": [1, 2] }, { "matches": ["a", "("] }] }), "/when", &mut |p, c, m| problems.push((p.to_string(), c, m)));
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert_eq!(problems[0].1, "unknown_operator");
    assert!(problems[0].2.contains("did you mean `gt`"));
    assert!(problems[1].0.ends_with("/between"));
    assert_eq!(problems[2].1, "bad_pattern");
}
