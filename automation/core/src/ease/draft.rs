//! What `flows.draft` and `flows.fix` give a model and read back: the format in short, the nodes
//! installed here, the answer's shape, the problems to correct, and the changes between two flows
//! (docs/automation/methods/flows.draft.md, flows.fix.md). The host asks the model; this crate
//! only writes and reads text.

use crate::expr::OPS;
use crate::format::tables::{self, Field, KINDS, TRIGGERS, TRIGGER_COMMON};
use crate::problem::Problem;
use serde_json::{json, Map, Value};

/// Nodes listed for the model, at most (the rest are named by count)
pub const MAX_NODES: usize = 300;
/// Input fields shown per node, at most
const MAX_FIELDS: usize = 12;

fn field_list(fields: &[Field]) -> String {
    fields.iter().map(|f| if f.required { format!("{}*", f.name) } else { f.name.to_string() }).collect::<Vec<_>>().join(", ")
}

/// The fields of an input schema in short: `repo*: string, title*: string, body: string`
pub fn schema_fields(schema: Option<&Value>) -> String {
    let Some(schema) = schema else { return String::new() };
    let required: Vec<&str> = schema.get("required").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else { return String::new() };
    let mut parts: Vec<String> = properties
        .iter()
        .take(MAX_FIELDS)
        .map(|(name, p)| {
            let ty = match p.get("type") {
                Some(Value::String(t)) => t.clone(),
                Some(Value::Array(types)) => types.iter().filter_map(Value::as_str).filter(|t| *t != "null").collect::<Vec<_>>().join("|"),
                _ if p.get("enum").is_some() => "enum".into(),
                _ => "any".into(),
            };
            let star = if required.contains(&name.as_str()) { "*" } else { "" };
            format!("{name}{star}: {ty}")
        })
        .collect();
    if properties.len() > MAX_FIELDS {
        parts.push(format!("… {} more", properties.len() - MAX_FIELDS));
    }
    parts.join(", ")
}

/// The required fields with their types, then the other fields' names: `path*: string; also
/// encoding, max_bytes`
fn required_fields(schema: Option<&Value>) -> String {
    let Some(schema) = schema else { return String::new() };
    let required: Vec<&str> = schema.get("required").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let names: Vec<&String> = schema.get("properties").and_then(Value::as_object).map(|p| p.keys().collect()).unwrap_or_default();
    let all = schema_fields(Some(schema));
    let mut out: Vec<String> = all.split(", ").filter(|p| required.iter().any(|r| p.starts_with(&format!("{r}*:")))).map(String::from).collect();
    let others: Vec<&str> = names.iter().map(|n| n.as_str()).filter(|n| !required.contains(n)).take(MAX_FIELDS).collect();
    if !others.is_empty() {
        out.push(format!("also {}", others.join(" ")));
    }
    out.join(", ")
}

fn node_line(node: &Value) -> String {
    let reference = node["ref"].as_str().unwrap_or_default();
    let description = node["description"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
    // Methods are many: shorter lines for them, their required fields only
    let method = node["step"] == "call";
    let description: String = description.chars().take(if method { 90 } else { 160 }).collect();
    let fields = if method { required_fields(node.get("input_schema")) } else { schema_fields(node.get("input_schema")) };
    let mut line = format!("- {} `{reference}`", node["step"].as_str().unwrap_or("tool"));
    if !description.is_empty() {
        line.push_str(&format!(": {description}"));
    }
    if !fields.is_empty() {
        line.push_str(&format!(" — with {{ {fields} }}"));
    }
    line
}

const EXAMPLE: &str = r#"{
  "flow": "cluaiz.flow/1",
  "name": "urgent-notes",
  "description": "Reads each new note in ~/Notes and keeps a list of the urgent ones",
  "on": [{ "file": { "path": "~/Notes", "glob": "*.md" } }],
  "steps": [
    { "id": "note", "call": "fs.read", "with": { "path": "${{ trigger.path }}" } },
    { "id": "judge", "ask": "Is this note urgent? Urgent means it needs action today.",
      "with": "${{ steps.note.output }}",
      "output_schema": { "type": "object", "properties": { "urgent": { "type": "boolean" }, "reason": { "type": "string" } }, "required": ["urgent"] } },
    { "id": "words", "set": { "count": { "expr": { "size": { "split": [{ "var": "steps.note.output.text" }, " "] } } } } },
    { "when": { "eq": [{ "var": "steps.judge.output.urgent" }, true] },
      "call": "fs.write",
      "with": { "path": "~/Notes/urgent/${{ trigger.name }}.md", "text": "${{ steps.judge.output.reason }} (${{ vars.count }} words)" } }
  ]
}"#;

/// The system text for writing a flow: the format in short, the nodes, the rules, an example
pub fn guide(nodes: &[Value]) -> String {
    let mut out = String::from(
        "You write automation flows for Cluaiz. A flow is one JSON object; answer with that object only, no other text.\n\n\
         # The file\n\
         - \"flow\": always \"cluaiz.flow/1\". \"name\": lowercase letters, digits, - and _ (1-64, starts with a letter or digit). \"description\": one line.\n\
         - \"on\": a list of triggers, each an object with one kind key. Leave it out for a flow that is run by hand.\n\
         - \"inputs\" (optional): input name → JSON Schema. \"outputs\" (optional): output name → value.\n\
         - \"steps\": the steps, run top to bottom. Each step is an object with exactly one kind key, plus common fields.\n\n\
         # Values\n\
         - \"${{ path }}\" inside a string reads a value: inputs.<name>, trigger.<field>, steps.<id>.output, vars.<name>, the `as` name inside each, now. A string that is only one reference keeps the value's type.\n\
         - A computed value (in with, set, outputs, output) is written {\"expr\": <operator object>}: {\"expr\": {\"size\": {\"var\": \"steps.list.output\"}}}. An operator object without expr is kept as it is, not computed.\n\
         - `set` is an object of names → values; each name is then read as vars.<name>. References always have two braces: ${{ vars.count }}, never ${vars.count}.\n\
         - Conditions (when, if, until, filter, stop_when) are JSON operator objects, never text: {\"gt\": [{\"var\": \"steps.count.output\"}, 0]}, {\"eq\": [{\"var\": \"steps.judge.output.urgent\"}, true]}, {\"all\": [...]}, {\"any\": [...]}, {\"not\": {...}}.\n\
         - Operators: ",
    );
    out.push_str(&OPS.iter().map(|o| o.name).collect::<Vec<_>>().join(", "));
    out.push_str("\n\n# Step kinds (the kind key's value, then its fields; * = required)\n");
    for kind in KINDS {
        let fields = field_list(kind.fields);
        if fields.is_empty() {
            out.push_str(&format!("- {}: {}\n", kind.key, kind.desc));
        } else {
            out.push_str(&format!("- {}: {}. Fields: {fields}\n", kind.key, kind.desc));
        }
    }
    out.push_str(&format!("Every step may have: {}. Give an id to each step whose output a later step reads.\n\n# Triggers\n", field_list(tables::COMMON)));
    for trigger in TRIGGERS {
        let fields = match trigger.ty {
            tables::Ty::Object(fields) => format!(" Object fields: {}", field_list(fields)),
            _ => String::new(),
        };
        let beside = if trigger.siblings.is_empty() { String::new() } else { format!(" Beside it: {}", field_list(trigger.siblings)) };
        out.push_str(&format!("- {}: {}.{fields}{beside}\n", trigger.key, trigger.desc));
    }
    out.push_str(&format!("Every trigger may have: {}. Examples: {{\"schedule\": \"0 9 * * 1-5\"}}, {{\"every\": \"15m\"}}, {{\"file\": {{\"path\": \"~/Downloads\", \"glob\": \"*.pdf\"}}}}.\n\n", field_list(TRIGGER_COMMON)));
    out.push_str("# What steps may use on this computer\nA `tool` step names a tool below; a `call` step names a method below. `with` gives the input fields shown.\n");
    for node in nodes.iter().take(MAX_NODES) {
        out.push_str(&node_line(node));
        out.push('\n');
    }
    if nodes.len() > MAX_NODES {
        out.push_str(&format!("({} more not listed)\n", nodes.len() - MAX_NODES));
    }
    out.push_str(
        "\n# Rules\n\
         - Use only the tools and methods listed above. When the request needs something that is not listed, leave that part out and say what is missing in \"description\".\n\
         - A judgement (\"is this urgent?\", \"what kind of mail is it?\") is an `ask` step with `output_schema`; a later step's `when` reads its answer.\n\
         - Do not write \"permissions\": they are worked out from the steps and confirmed by the owner.\n\
         - Leave \"model\" out of `ask` and `agent` steps: the default chat model answers. Write a model only when the request names one, exactly as named.\n\
         - Paths may start with ~ for the home folder. Secrets are written env:NAME and listed in \"secrets\".\n\n\
         # Example\n",
    );
    out.push_str(EXAMPLE);
    out
}

/// A flow as file text
pub fn text_of(flow: &Value, format: crate::format::Format) -> String {
    match format {
        crate::format::Format::Json => serde_json::to_string_pretty(flow).unwrap_or_default() + "\n",
        crate::format::Format::Yaml => serde_yaml::to_string(flow).unwrap_or_default(),
    }
}

/// The answer's shape for a draft: enough for a model held to a schema to write a whole flow
/// (the validator checks the rest)
pub fn flow_answer_schema() -> Value {
    json!({
        "type": "object",
        "required": ["flow", "name", "steps"],
        "properties": {
            "flow": { "type": "string", "enum": [tables::VERSION] },
            "name": { "type": "string" },
            "description": { "type": "string" },
            "on": { "type": "array", "items": { "type": "object" } },
            "inputs": { "type": "object" },
            "outputs": { "type": "object" },
            "secrets": { "type": "array", "items": { "type": "string" } },
            "steps": { "type": "array", "minItems": 1, "items": { "type": "object" } }
        }
    })
}

/// The answer's shape for a fix
pub fn fix_answer_schema() -> Value {
    json!({
        "type": "object",
        "required": ["diagnosis", "cause_outside_flow", "reasons", "flow"],
        "properties": {
            "diagnosis": { "type": "string" },
            "cause_outside_flow": { "type": "boolean" },
            "reasons": { "type": "array", "items": { "type": "object", "required": ["path", "why"], "properties": { "path": { "type": "string" }, "why": { "type": "string" } } } },
            "flow": { "type": "object" }
        }
    })
}

/// The system text for a fix: the guide, and what to answer
pub fn fix_guide(nodes: &[Value]) -> String {
    let guide = guide(nodes).replace("A flow is one JSON object; answer with that object only, no other text.", "Here you correct a flow.");
    format!(
        "{guide}\n\n# Your answer\nOne JSON object, nothing else: {{ \"diagnosis\": what went wrong in plain words, \"cause_outside_flow\": true when nothing in the flow can fix it (a missing key, a tool that is down, a full disk; then return the flow unchanged), \"reasons\": [{{ \"path\": the JSON Pointer of each part you changed, such as /steps/1/with/repo, \"why\": one line }}], \"flow\": the whole corrected flow }}. Change as little as you can; keep the name, ids and everything that works."
    )
}

/// The model's answer as JSON: the object in it, without code fences or text around it. YAML is
/// read too. `None` when there is no object.
pub fn read_answer(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    let body = match trimmed.find("```") {
        Some(start) => {
            let after = &trimmed[start + 3..];
            let after = after.split_once('\n').map(|(lang, rest)| if lang.trim().chars().all(|c| c.is_ascii_alphanumeric()) { rest } else { after }).unwrap_or(after);
            after.split("```").next().unwrap_or(after)
        }
        None => trimmed,
    };
    if let Ok(value @ Value::Object(_)) = serde_json::from_str::<Value>(body.trim()) {
        return Some(value);
    }
    if let (Some(start), Some(end)) = (body.find('{'), body.rfind('}')) {
        if start < end {
            if let Ok(value @ Value::Object(_)) = serde_json::from_str::<Value>(&body[start..=end]) {
                return Some(value);
            }
        }
    }
    match serde_yaml::from_str::<Value>(body) {
        Ok(value @ Value::Object(_)) => Some(value),
        _ => None,
    }
}

/// A draft answer as a flow: the object itself, or the flow inside `{ "flow": { … } }`
pub fn flow_of(answer: Value) -> Value {
    match answer.get("flow") {
        Some(Value::Object(inner)) if inner.contains_key("steps") => Value::Object(inner.clone()),
        _ => answer,
    }
}

/// A flow with nothing in it where there must be something: no steps, or a step or trigger that
/// is an empty object. Some providers answer so when held to a schema with open objects.
pub fn hollow(flow: &Value) -> bool {
    let empty = |v: &Value| v.as_object().is_some_and(Map::is_empty);
    let steps = flow.get("steps").and_then(Value::as_array);
    steps.is_none_or(|s| s.is_empty() || s.iter().any(empty)) || flow.get("on").and_then(Value::as_array).is_some_and(|t| t.iter().any(empty))
}

/// The problems for the model to correct, one per line
pub fn correction(problems: &[Problem]) -> String {
    let mut out = String::from("The flow has these problems:\n");
    for problem in problems.iter().filter(|p| p.is_error()) {
        let place = if problem.path.is_empty() { "the file".to_string() } else { problem.path.clone() };
        out.push_str(&format!("- {place}: {}", problem.message));
        if !problem.fix.is_null() {
            out.push_str(&format!(" (use {})", problem.fix));
        }
        out.push('\n');
    }
    out.push_str("Write the whole flow again with them corrected, as one JSON object.");
    out
}

/// Nodes a flow names that are not installed: `[{ need, path, suggestions }]`
pub fn missing(problems: &[Problem]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for problem in problems.iter().filter(|p| p.code == "unknown_node") {
        let need = problem.message.split('`').nth(1).unwrap_or_default().to_string();
        if out.iter().any(|m| m["need"] == json!(need)) {
            continue;
        }
        out.push(json!({ "need": need, "path": problem.path, "suggestions": [] }));
    }
    out
}

/// Only problems that a missing node explains are left
pub fn only_missing(problems: &[Problem]) -> bool {
    problems.iter().filter(|p| p.is_error()).all(|p| p.code == "unknown_node")
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn diff(path: &str, before: &Value, after: &Value, out: &mut Vec<Value>) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().collect();
            keys.extend(b.keys().filter(|k| !a.contains_key(*k)));
            for key in keys {
                let at = format!("{path}/{}", escape(key));
                diff(&at, a.get(key).unwrap_or(&Value::Null), b.get(key).unwrap_or(&Value::Null), out);
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                diff(&format!("{path}/{i}"), x, y, out);
            }
        }
        _ => out.push(json!({ "path": if path.is_empty() { "/" } else { path }, "before": before, "after": after })),
    }
}

/// The changes from one flow to another, each `{ path, before, after }` with a JSON Pointer. A
/// list whose length changed is one change.
pub fn changes(before: &Value, after: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    diff("", before, after, &mut out);
    out
}

/// Gives each change the model's reason for it: the reason whose path is the change's path or
/// the closest part above it. A renamed field is two changes (one removed, one added beside it);
/// the one without a reason takes the other's.
pub fn with_reasons(changes: Vec<Value>, reasons: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = changes
        .into_iter()
        .map(|mut change| {
            let path = change["path"].as_str().unwrap_or_default().to_string();
            let why = reasons
                .iter()
                .filter_map(|r| Some((r.get("path")?.as_str()?, r.get("why")?.as_str()?)))
                .filter(|(p, _)| path == *p || path.starts_with(&format!("{}/", p.trim_end_matches('/'))) || p.starts_with(&format!("{path}/")))
                .max_by_key(|(p, _)| p.len())
                .map(|(_, w)| w.to_string());
            change["why"] = json!(why);
            change
        })
        .collect();
    let parent = |c: &Value| c["path"].as_str().and_then(|p| p.rsplit_once('/')).map(|(p, _)| p.to_string());
    for i in 0..out.len() {
        if !out[i]["why"].is_null() {
            continue;
        }
        let added = out[i]["before"].is_null();
        let partner = out.iter().enumerate().find(|(j, other)| {
            *j != i && !other["why"].is_null() && parent(other) == parent(&out[i]) && other["after"].is_null() == added && other["before"].is_null() != added
        });
        if let Some((_, other)) = partner {
            let why = other["why"].clone();
            out[i]["why"] = why;
        }
    }
    out
}

/// Sample trigger data for a test of a draft, from its first trigger
pub fn sample_trigger(flow: &Value) -> Value {
    let specs = crate::trigger::specs(flow);
    let Some(spec) = specs.first() else { return json!({ "kind": "manual", "by": "draft" }) };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut data = match spec.kind {
        "schedule" => json!({ "scheduled_for": now }),
        "webhook" => json!({ "method": "POST", "headers": { "content-type": "application/json" }, "query": {}, "body": {}, "remote": "127.0.0.1" }),
        "poll" => json!({ "output": null, "previous": null, "new_items": [] }),
        "file" => {
            let folder = spec.field("path").and_then(Value::as_str).unwrap_or("~").trim_end_matches(['/', '\\']).to_string();
            let extension = spec.field("glob").and_then(Value::as_str).and_then(|g| g.rsplit_once('.')).map(|(_, e)| e.trim_end_matches('*').to_string()).filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or_else(|| "txt".into());
            json!({ "path": format!("{folder}/sample.{extension}"), "name": "sample", "extension": extension, "event": "created", "size": 0, "modified_at": now })
        }
        "engine" => json!({ "event": spec.body(), "data": {} }),
        "event" => json!({ "name": spec.body(), "data": {}, "from": { "flow": "sample", "scope": "user", "run_id": "run_sample" }, "hops": 1 }),
        "tool" => json!({ "caller": { "kind": "agent" } }),
        _ => json!({}),
    };
    data["kind"] = json!(spec.kind);
    data["id"] = json!(spec.id);
    data["at"] = json!(now);
    data
}

/// Sample inputs for a test: each input's default, or a value of its type
pub fn sample_inputs(flow: &Value) -> Value {
    let mut out = Map::new();
    for (name, schema) in flow.get("inputs").and_then(Value::as_object).into_iter().flatten() {
        let value = schema.get("default").cloned().unwrap_or_else(|| match schema.get("enum").and_then(Value::as_array).and_then(|l| l.first()) {
            Some(first) => first.clone(),
            None => match schema.get("type").and_then(Value::as_str) {
                Some("number") | Some("integer") => json!(1),
                Some("boolean") => json!(false),
                Some("array") => json!([]),
                Some("object") => json!({}),
                _ => json!("sample"),
            },
        });
        out.insert(name.clone(), value);
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_read_with_or_without_fences() {
        assert_eq!(read_answer("```json\n{\"a\": 1}\n```").unwrap(), json!({ "a": 1 }));
        assert_eq!(read_answer("Here it is: {\"a\": {\"b\": 2}} done").unwrap(), json!({ "a": { "b": 2 } }));
        assert_eq!(read_answer("flow: cluaiz.flow/1\nname: x\nsteps: []\n").unwrap()["name"], "x");
        assert!(read_answer("no object here").is_none());
        assert_eq!(flow_of(json!({ "flow": { "flow": "cluaiz.flow/1", "steps": [] } }))["flow"], "cluaiz.flow/1");
    }

    #[test]
    fn changes_are_json_pointers_with_reasons() {
        let before = json!({ "steps": [{ "id": "a", "with": { "repo": "my/old" } }, { "id": "b" }] });
        let after = json!({ "steps": [{ "id": "a", "with": { "repo": "my/new" } }, { "id": "b", "retry": { "max": 3 } }] });
        let list = with_reasons(changes(&before, &after), &[json!({ "path": "/steps/0/with/repo", "why": "renamed" })]);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], json!({ "path": "/steps/0/with/repo", "before": "my/old", "after": "my/new", "why": "renamed" }));
        assert_eq!(list[1]["path"], "/steps/1/retry");
        assert_eq!(list[1]["why"], Value::Null);
        // A list of another length is one change
        assert_eq!(changes(&json!({ "s": [1] }), &json!({ "s": [1, 2] }))[0]["path"], "/s");
        // A renamed field: both halves carry the reason
        let renamed = with_reasons(
            changes(&json!({ "steps": [{ "wiht": { "a": 1 } }] }), &json!({ "steps": [{ "with": { "a": 1 } }] })),
            &[json!({ "path": "/steps/0/wiht", "why": "typo" })],
        );
        assert_eq!(renamed.len(), 2);
        assert!(renamed.iter().all(|c| c["why"] == "typo"), "{renamed:?}");
    }

    #[test]
    fn the_guide_names_every_kind_and_the_nodes() {
        let nodes = vec![json!({ "ref": "fs.read", "step": "call", "description": "Reads a file", "input_schema": { "type": "object", "required": ["path"], "properties": { "path": { "type": "string" } } } })];
        let text = guide(&nodes);
        for kind in KINDS {
            assert!(text.contains(&format!("- {}:", kind.key)), "{}", kind.key);
        }
        assert!(text.contains("- call `fs.read`: Reads a file — with { path*: string }"));
    }

    #[test]
    fn hollow_answers_are_seen() {
        assert!(hollow(&json!({ "flow": "cluaiz.flow/1", "on": [{}], "steps": [{ "set": {} }] })));
        assert!(hollow(&json!({ "steps": [{}, {}] })));
        assert!(hollow(&json!({ "name": "x" })));
        assert!(!hollow(&serde_json::from_str(EXAMPLE).unwrap()));
    }

    #[test]
    fn the_example_is_a_valid_flow() {
        let flow: Value = serde_json::from_str(EXAMPLE).unwrap();
        let report = crate::validate::validate(&flow, None, &crate::host::NoCatalog);
        assert!(report.valid, "{:?}", report.problems);
    }

    #[test]
    fn samples_follow_the_trigger_and_inputs() {
        let flow = json!({ "on": [{ "file": { "path": "~/Downloads/", "glob": "*.pdf" } }], "inputs": { "n": { "type": "integer" }, "who": { "type": "string", "default": "me" } } });
        let trigger = sample_trigger(&flow);
        assert_eq!(trigger["path"], "~/Downloads/sample.pdf");
        assert_eq!(trigger["kind"], "file");
        assert_eq!(sample_inputs(&flow), json!({ "n": 1, "who": "me" }));
        assert_eq!(sample_trigger(&json!({}))["kind"], "manual");
    }
}
