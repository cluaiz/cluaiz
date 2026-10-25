//! `rpc.discover`: an OpenRPC document (https://spec.open-rpc.org/) built from the registry, showing
//! a caller only the methods it may call (01-architecture §7.6)

use crate::principal::{Principal, Scope};
use crate::registry::Registry;
use serde_json::{json, Map, Value};

/// The OpenRPC version the document follows
const OPENRPC: &str = "1.3.2";
/// Bumped only for a protocol-level break (`/v2/cep`)
pub const CEP_VERSION: u32 = 1;

struct Entry {
    group: String,
    scope: Scope,
    owner_only: bool,
    doc: Value,
}

pub struct Discovery {
    info: Value,
    entries: Vec<Entry>,
    schemas: Map<String, Value>,
}

/// Moves `$defs` out of a schema into `schemas` and points its refs at `#/components/schemas/`
fn lift(mut schema: Value, schemas: &mut Map<String, Value>) -> Value {
    fn retarget(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(target)) = map.get_mut("$ref") {
                    if let Some(name) = target.strip_prefix("#/$defs/") {
                        *target = format!("#/components/schemas/{name}");
                    }
                }
                map.values_mut().for_each(retarget);
            }
            Value::Array(items) => items.iter_mut().for_each(retarget),
            _ => {}
        }
    }
    if let Some(Value::Object(defs)) = schema.as_object_mut().and_then(|s| s.remove("$defs")) {
        for (name, mut def) in defs {
            retarget(&mut def);
            schemas.insert(name, def);
        }
    }
    if let Some(map) = schema.as_object_mut() {
        map.remove("$schema");
    }
    retarget(&mut schema);
    schema
}

impl Discovery {
    pub fn build<Ctx: Send + 'static>(registry: &Registry<Ctx>, title: &str, version: &str) -> Self {
        let mut schemas = Map::new();
        let mut entries = Vec::new();
        for method in registry.methods() {
            let params = lift(method.params_schema.clone(), &mut schemas);
            let required: Vec<&str> =
                params["required"].as_array().map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let descriptors: Vec<Value> = params["properties"]
                .as_object()
                .map(|props| {
                    props
                        .iter()
                        .map(|(name, schema)| json!({ "name": name, "required": required.contains(&name.as_str()), "schema": schema }))
                        .collect()
                })
                .unwrap_or_default();
            let example_params: Vec<Value> = method
                .meta
                .example_params
                .as_object()
                .map(|p| p.iter().map(|(name, value)| json!({ "name": name, "value": value })).collect())
                .unwrap_or_default();
            let meta = &method.meta;
            let doc = json!({
                "name": method.name,
                "summary": meta.summary,
                "paramStructure": "by-name",
                "params": descriptors,
                "result": { "name": "result", "schema": lift(method.result_schema.clone(), &mut schemas) },
                "examples": [{
                    "name": "example",
                    "params": example_params,
                    "result": { "name": "result", "value": meta.example_result },
                }],
                "x-cep": {
                    "scope": meta.scope,
                    "execution": meta.execution,
                    "destructive": meta.destructive,
                    "idempotent": meta.idempotent,
                    "mcp": meta.mcp,
                    "owner_only": meta.owner_only,
                },
            });
            let group = method.name.split('.').next().unwrap_or_default().to_string();
            entries.push(Entry { group, scope: meta.scope, owner_only: meta.owner_only, doc });
        }
        let info = json!({ "title": title, "version": version, "x-cep-version": CEP_VERSION });
        Self { info, entries, schemas }
    }

    /// The document for one caller: only what it may call, and only `group` when given
    pub fn document(&self, principal: &Principal, group: Option<&str>) -> Value {
        let methods: Vec<&Value> = self
            .entries
            .iter()
            .filter(|e| principal.allow(e.scope, e.owner_only).is_ok())
            .filter(|e| group.is_none_or(|g| e.group == g))
            .map(|e| &e.doc)
            .collect();
        json!({
            "openrpc": OPENRPC,
            "info": self.info,
            "methods": methods,
            "components": { "schemas": self.schemas },
        })
    }
}
