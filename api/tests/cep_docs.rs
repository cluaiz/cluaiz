//! The CEP reference in `docs/api/cep/` comes from the method registry (`rpc.discover`), so the
//! docs and the code cannot drift. Each method has its own page,
//! `docs/api/cep/<category>/<group>/<method>.md`: the block between the two `---` lines is
//! generated (the payload data an app reads: params, request, response, errors), the text under it
//! is written by hand and kept. `openrpc.json` and each category's `README.md` index are generated.
//!
//! Check (fails when a page is stale):  cargo test --release -p cluaiz-api --test cep_docs
//! Write:                               CEP_DOCS_WRITE=1 cargo test --release -p cluaiz-api --test cep_docs

mod support;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// The five categories (the App's API sidebar) and the CEP groups in each
const CATEGORIES: [(&str, &str, &[&str]); 5] = [
    ("ai-inference", "AI & Inference Core", &["chat", "audio", "memory", "notes", "context", "kv"]),
    ("agents-tools", "Agents, MCP & Tools", &["tools", "approvals"]),
    ("filesystem", "Filesystem & Security Jail", &["fs", "workspace"]),
    ("models", "Models & Engine Vault", &["models"]),
    ("system", "System & Infrastructure", &["system", "config", "storage", "tokens", "tasks", "rpc"]),
];

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/api/cep")
}

fn category_of(group: &str) -> Result<(&'static str, &'static str)> {
    CATEGORIES
        .iter()
        .find(|(_, _, groups)| groups.contains(&group))
        .map(|(folder, title, _)| (*folder, *title))
        .ok_or_else(|| anyhow!("CEP group `{group}` has no docs category: add it to CATEGORIES in api/tests/cep_docs.rs"))
}

/// A JSON string is a valid YAML double-quoted scalar
fn quoted(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

fn resolve<'a>(schema: &'a Value, schemas: &'a Map<String, Value>) -> &'a Value {
    match schema.get("$ref").and_then(Value::as_str) {
        Some(reference) => schemas.get(reference.rsplit('/').next().unwrap_or_default()).unwrap_or(schema),
        None => schema,
    }
}

/// A short, readable type: `string`, `integer | null`, `Kind`, `string[]`, `"a" | "b"`
fn type_name(schema: &Value, schemas: &Map<String, Value>) -> String {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or_default();
        let target = resolve(schema, schemas);
        if let Some(values) = target.get("enum").and_then(Value::as_array) {
            return values.iter().map(Value::to_string).collect::<Vec<_>>().join(" | ");
        }
        if let Some(options) = target.get("oneOf").and_then(Value::as_array) {
            let consts: Vec<String> = options.iter().filter_map(|o| o.get("const").or(o.get("enum").and_then(|e| e.get(0)))).map(Value::to_string).collect();
            if consts.len() == options.len() {
                return consts.join(" | ");
            }
        }
        if target.get("properties").is_some() {
            return "object".into();
        }
        if target.get("anyOf").is_some() || target.get("oneOf").is_some() {
            return type_name(target, schemas);
        }
        return name.to_string();
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(options) = schema.get(key).and_then(Value::as_array) {
            let mut names: Vec<String> = options.iter().map(|o| type_name(o, schemas)).filter(|n| n != "null").collect();
            if options.iter().any(|o| o.get("type").and_then(Value::as_str) == Some("null")) {
                names.push("null".into());
            }
            return names.join(" | ");
        }
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values.iter().map(Value::to_string).collect::<Vec<_>>().join(" | ");
    }
    match schema.get("type") {
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" | "),
        Some(Value::String(kind)) if kind == "array" => format!("{}[]", type_name(schema.get("items").unwrap_or(&Value::Null), schemas)),
        Some(Value::String(kind)) => kind.clone(),
        _ if schema == &Value::Bool(true) || schema.as_object().is_some_and(Map::is_empty) => "any".into(),
        _ => "any".into(),
    }
}

fn description(schema: &Value, schemas: &Map<String, Value>) -> String {
    schema
        .get("description")
        .or_else(|| resolve(schema, schemas).get("description"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .replace('\n', " ")
}

/// The object a schema stands for (through `$ref` and a nullable `anyOf`), when it has fields
fn object_of<'a>(schema: &'a Value, schemas: &'a Map<String, Value>) -> Option<&'a Value> {
    let target = resolve(schema, schemas);
    if target.get("properties").is_some() {
        return Some(target);
    }
    target
        .get("anyOf")
        .and_then(Value::as_array)?
        .iter()
        .map(|option| resolve(option, schemas))
        .find(|option| option.get("properties").is_some())
}

/// `params:` entries; nested objects list their own fields (two levels)
fn fields(lines: &mut Vec<String>, indent: usize, object: &Value, examples: &Value, schemas: &Map<String, Value>, depth: usize) {
    let required: Vec<&str> = object["required"].as_array().map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let Some(properties) = object["properties"].as_object() else { return };
    let pad = " ".repeat(indent);
    for (name, schema) in properties {
        lines.push(format!("{pad}- name: {}", quoted(name)));
        lines.push(format!("{pad}  type: {}", quoted(&type_name(schema, schemas))));
        lines.push(format!("{pad}  req: {}", required.contains(&name.as_str())));
        lines.push(format!("{pad}  desc: {}", quoted(&description(schema, schemas))));
        lines.push(format!("{pad}  example: {}", examples.get(name).unwrap_or(&Value::Null)));
        if depth < 2 {
            if let Some(nested) = object_of(schema, schemas) {
                lines.push(format!("{pad}  fields:"));
                fields(lines, indent + 4, nested, examples.get(name).unwrap_or(&Value::Null), schemas, depth + 1);
            }
        }
    }
}

fn block(lines: &mut Vec<String>, key: &str, value: &Value) {
    lines.push(format!("{key}: |"));
    for line in serde_json::to_string_pretty(value).unwrap_or_default().lines() {
        lines.push(format!("  {line}"));
    }
}

/// The generated `---` block of one method's page
fn front_matter(method: &Value, schemas: &Map<String, Value>) -> Result<String> {
    let name = method["name"].as_str().context("method name")?;
    let group = name.split('.').next().unwrap_or_default();
    let (category, category_title) = category_of(group)?;
    let cep = &method["x-cep"];
    let example = &method["examples"][0];
    let mut params = Map::new();
    for param in example["params"].as_array().into_iter().flatten() {
        params.insert(param["name"].as_str().unwrap_or_default().to_string(), param["value"].clone());
    }
    let mut lines = vec!["---".to_string()];
    lines.push(format!("method: {}", quoted(name)));
    lines.push(format!("transport: {}", quoted("POST /v1/cep")));
    lines.push(format!("category: {}", quoted(category)));
    lines.push(format!("category_title: {}", quoted(category_title)));
    lines.push(format!("group: {}", quoted(group)));
    lines.push(format!("summary: {}", quoted(method["summary"].as_str().unwrap_or_default())));
    for key in ["scope", "execution"] {
        lines.push(format!("{key}: {}", quoted(cep[key].as_str().unwrap_or_default())));
    }
    for key in ["mcp", "owner_only", "destructive", "idempotent"] {
        lines.push(format!("{key}: {}", cep[key].as_bool().unwrap_or(false)));
    }
    let object = json!({
        "properties": method["params"].as_array().into_iter().flatten()
            .map(|p| (p["name"].as_str().unwrap_or_default().to_string(), p["schema"].clone())).collect::<Map<_, _>>(),
        "required": method["params"].as_array().into_iter().flatten()
            .filter(|p| p["required"] == true).map(|p| p["name"].clone()).collect::<Vec<_>>(),
    });
    if object["properties"].as_object().is_some_and(Map::is_empty) {
        lines.push("params: []".into());
    } else {
        lines.push("params:".into());
        fields(&mut lines, 2, &object, &Value::Object(params.clone()), schemas, 0);
    }
    block(&mut lines, "request_payload", &json!({ "jsonrpc": "2.0", "id": 1, "method": name, "params": params }));
    block(&mut lines, "response", &json!({ "jsonrpc": "2.0", "id": 1, "result": example["result"]["value"] }));
    let errors = method["errors"].as_array().cloned().unwrap_or_default();
    if errors.is_empty() {
        lines.push("errors: []".into());
    } else {
        lines.push("errors:".into());
        for error in errors {
            lines.push(format!("  - code: {}", error["code"]));
            lines.push(format!("    type: {}", quoted(error["data"]["type"].as_str().unwrap_or_default())));
            lines.push(format!("    when: {}", quoted(error["message"].as_str().unwrap_or_default())));
        }
    }
    lines.push("---".into());
    Ok(lines.join("\n") + "\n")
}

/// The hand-written part of an existing page (after the generated block)
fn body_of(page: &str) -> Option<String> {
    let page = page.replace("\r\n", "\n");
    let rest = page.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    Some(rest[end + 5..].to_string())
}

fn category_index(dir: &Path, title: &str, folder: &str, groups: &[&str], methods: &[&Value]) -> String {
    let mut lines = vec![
        format!("# {title}"),
        String::new(),
        "<!-- Generated from the CEP registry by api/tests/cep_docs.rs; do not edit. -->".to_string(),
        String::new(),
    ];
    let overviews: Vec<String> = groups
        .iter()
        .filter(|group| dir.join(folder).join(group).join("README.md").exists())
        .map(|group| format!("[`{group}.*` overview](./{group}/README.md)"))
        .collect();
    if !overviews.is_empty() {
        lines.push(format!("How it works: {}.", overviews.join(", ")));
        lines.push(String::new());
    }
    lines.extend([
        "| Method | Scope | Runs as | Summary |".to_string(),
        "|---|---|---|---|".to_string(),
    ]);
    for method in methods {
        let name = method["name"].as_str().unwrap_or_default();
        let group = name.split('.').next().unwrap_or_default();
        lines.push(format!(
            "| [`{name}`](./{group}/{name}.md) | {} | {} | {} |",
            method["x-cep"]["scope"].as_str().unwrap_or_default(),
            method["x-cep"]["execution"].as_str().unwrap_or_default(),
            method["summary"].as_str().unwrap_or_default().replace('|', "\\|"),
        ));
    }
    lines.push(String::new());
    lines.push(format!("All categories: [CEP reference](../README.md). Folder: `docs/api/cep/{folder}/`."));
    lines.join("\n") + "\n"
}

#[tokio::test]
async fn test_cep_docs_match_the_registry() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let router = cluaiz_api::create_router(cluaiz_api::AppState::new_test_state(workspace.path().to_owned()));
    let discovery = support::cep(&router, "rpc.discover", json!({})).await?.map_err(|e| anyhow!("rpc.discover: {e}"))?;
    let schemas = discovery["components"]["schemas"].as_object().cloned().unwrap_or_default();
    let methods = discovery["methods"].as_array().context("methods")?.clone();
    let write = std::env::var_os("CEP_DOCS_WRITE").is_some();
    let dir = docs_dir();
    let mut expected: Vec<(PathBuf, String)> = Vec::new();

    expected.push((dir.join("openrpc.json"), serde_json::to_string_pretty(&discovery)? + "\n"));
    for (folder, title, groups) in CATEGORIES {
        let in_category: Vec<&Value> = methods
            .iter()
            .filter(|m| groups.contains(&m["name"].as_str().unwrap_or_default().split('.').next().unwrap_or_default()))
            .collect();
        expected.push((dir.join(folder).join("README.md"), category_index(&dir, title, folder, groups, &in_category)));
    }
    let mut missing_body = Vec::new();
    for method in &methods {
        let name = method["name"].as_str().context("method name")?;
        let group = name.split('.').next().unwrap_or_default();
        let (folder, _) = category_of(group)?;
        let path = dir.join(folder).join(group).join(format!("{name}.md"));
        let body = std::fs::read_to_string(&path).ok().and_then(|page| body_of(&page)).unwrap_or_default();
        if body.trim().lines().filter(|l| !l.trim().is_empty() && !l.starts_with("# ")).count() == 0 {
            missing_body.push(name.to_string());
        }
        let body = if body.trim().is_empty() { format!("\n# {name}\n") } else { body };
        expected.push((path, front_matter(method, &schemas)? + &body));
    }

    let mut stale = Vec::new();
    for (path, text) in &expected {
        let current = std::fs::read_to_string(path).unwrap_or_default().replace("\r\n", "\n");
        if &current != text {
            if write {
                std::fs::create_dir_all(path.parent().context("parent")?)?;
                std::fs::write(path, text)?;
            } else {
                stale.push(path.strip_prefix(&dir).unwrap_or(path).display().to_string());
            }
        }
    }
    if !stale.is_empty() {
        bail!("{} CEP doc file(s) are stale; run CEP_DOCS_WRITE=1 cargo test --release -p cluaiz-api --test cep_docs\n{}", stale.len(), stale.join("\n"));
    }
    if !write && !missing_body.is_empty() {
        bail!("These method pages have no hand-written explanation under their `---` block: {}", missing_body.join(", "));
    }
    // The docs site's navigation must list every method page
    let registry = std::fs::read_to_string(dir.join("../../registry.json"))?;
    let unlisted: Vec<String> = methods
        .iter()
        .filter_map(|m| {
            let name = m["name"].as_str()?;
            let group = name.split('.').next()?;
            let path = format!("docs/api/cep/{}/{group}/{name}.md", category_of(group).ok()?.0);
            (!registry.contains(&path)).then_some(path)
        })
        .collect();
    if !unlisted.is_empty() {
        bail!("docs/registry.json does not list these method pages:\n{}", unlisted.join("\n"));
    }
    Ok(())
}
