//! Compile the production methods and run them through the real CEP dispatcher.
//! The fixture supplies only the workspace field these methods need, not EngineHost.
//! Real filesystem, real principal/approval stores and real ToolsEngine workspace;
//! this is method acceptance, not HTTP/AppState/native transport acceptance.

#[path = "../../../api/src/cep/methods/fs.rs"]
mod files;
#[path = "../../../api/src/cep/roots.rs"]
pub mod roots;
#[path = "../../../api/src/cep/methods/workspace.rs"]
mod workspace;

mod cep {
    pub use crate::roots;
    use std::{path::PathBuf, sync::Arc};
    use tokio::sync::RwLock;

    pub struct WorkspaceState {
        pub current_workspace_dir: Arc<RwLock<PathBuf>>,
    }

    #[derive(Clone)]
    pub struct CallCtx {
        pub state: Arc<WorkspaceState>,
    }
}

use cluaiz_cep::{ApprovalStore, Engine, Limits, OnDenied, Principal, Registry, Scope, TaskStore};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc, time::Duration};

struct Fixture {
    root: tempfile::TempDir,
    engine: Engine<cep::CallCtx>,
    ctx: cep::CallCtx,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let ctx = cep::CallCtx {
            state: Arc::new(cep::WorkspaceState {
                current_workspace_dir: Arc::new(tokio::sync::RwLock::new(
                    root.path().to_path_buf(),
                )),
            }),
        };
        let mut registry = Registry::new();
        files::register(&mut registry);
        workspace::register(&mut registry);
        let engine = Engine::new(
            registry,
            ApprovalStore::new(Duration::from_secs(60)),
            TaskStore::new(8, 60_000, 60_000),
            Limits::default(),
            "Filesystem contract",
            "test",
        );
        Self { root, engine, ctx }
    }

    fn principal(&self, scopes: Vec<Scope>) -> Arc<Principal> {
        Arc::new(Principal {
            id: "token:fs-contract".into(),
            scopes,
            roots: Some(vec![fs::canonicalize(self.root.path()).unwrap()]),
            on_denied: OnDenied::Error,
            owner: false,
        })
    }

    async fn call(&self, principal: Arc<Principal>, method: &str, params: Value) -> Value {
        let request =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
                .unwrap();
        serde_json::from_slice(
            &self
                .engine
                .call(self.ctx.clone(), principal, &request)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    async fn ok(&self, method: &str, params: Value) -> Value {
        let reply = self
            .call(Arc::new(Principal::owner()), method, params)
            .await;
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        assert!(reply.get("result").is_some(), "{method}: {reply}");
        reply["result"].clone()
    }
}

#[tokio::test]
async fn test_file_methods_round_trip_real_unicode_and_binary_data() {
    let f = Fixture::new();
    let written = f
        .ok("fs.write", json!({"path":"नाम.md","text":"नमस्ते"}))
        .await;
    assert_eq!(written["bytes"], "नमस्ते".len());
    let read = f.ok("fs.read", json!({"path":"नाम.md"})).await;
    assert_eq!(read["text"], "नमस्ते");
    assert_eq!(read["binary"], false);
    assert!(read.get("status").is_none());
    f.ok(
        "fs.write",
        json!({"path":"binary.dat","data_base64":"AP8BgA=="}),
    )
    .await;
    let binary = f.ok("fs.read", json!({"path":"binary.dat"})).await;
    assert_eq!(binary["data_base64"], "AP8BgA==");
    assert_eq!(binary["binary"], true);
    assert_eq!(binary["size"], 4);
    f.ok("fs.mkdir", json!({"path":"nested/sub"})).await;
    f.ok("fs.copy", json!({"from":"नाम.md","to":"nested/copy.md"}))
        .await;
    f.ok(
        "fs.rename",
        json!({"from":"nested/copy.md","to":"nested/renamed.md"}),
    )
    .await;
    let listing = f.ok("fs.list", json!({"recursive":true})).await;
    assert!(listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["path"] == "nested/renamed.md"));
    for item in listing["items"].as_array().unwrap() {
        assert!(item.get("size").is_some() && item.get("is_folder").is_some());
        assert!(item.get("length").is_none() && item.get("isFolder").is_none());
    }
    f.ok("fs.delete", json!({"path":"nested"})).await;
    assert!(!f.root.path().join("nested").exists());
}

#[tokio::test]
async fn test_invalid_write_payload_has_no_filesystem_side_effects() {
    let f = Fixture::new();
    for payload in [
        json!({"path":"new/file.txt"}),
        json!({"path":"new/file.txt","text":"a","data_base64":"Yg=="}),
        json!({"path":"new/file.txt","data_base64":"%%%"}),
        json!({"path":"new/file.txt","text":"a","caller":"user"}),
    ] {
        let reply = f
            .call(Arc::new(Principal::owner()), "fs.write", payload)
            .await;
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
        assert!(!f.root.path().join("new").exists());
    }
}

#[tokio::test]
async fn test_scopes_and_host_picker_are_enforced_before_side_effects() {
    let f = Fixture::new();
    let reader = f.principal(vec![Scope::Read]);
    let denied = f
        .call(
            reader.clone(),
            "fs.write",
            json!({"path":"blocked.txt","text":"data"}),
        )
        .await;
    assert_eq!(denied["error"]["code"], -32002);
    assert_eq!(denied["error"]["data"]["needs"], "write");
    assert!(!f.root.path().join("blocked.txt").exists());
    // The negative call must return before any native UI is opened.
    let picker = f.call(reader, "fs.pick_folder", json!({})).await;
    assert_eq!(picker["error"]["code"], -32002);
    assert_eq!(picker["error"]["data"]["needs"], "owner");
}

#[tokio::test]
async fn test_traversal_and_missing_reads_return_typed_errors() {
    let f = Fixture::new();
    let absent = f
        .call(
            Arc::new(Principal::owner()),
            "fs.read",
            json!({"path":"missing"}),
        )
        .await;
    assert_eq!(absent["error"]["code"], -32004);
    assert_eq!(absent["error"]["data"]["type"], "file_not_found");
    let outside = f
        .root
        .path()
        .parent()
        .unwrap()
        .join(format!("cep-not-created-{}", uuid::Uuid::new_v4()));
    let denied = f
        .call(
            f.principal(vec![Scope::Write]),
            "fs.write",
            json!({"path":outside,"text":"blocked"}),
        )
        .await;
    assert_eq!(denied["error"]["code"], -32002);
    assert!(!outside.exists());
}

#[tokio::test]
async fn test_copy_rejects_self_descendant_and_preserves_original() {
    let f = Fixture::new();
    f.ok("fs.mkdir", json!({"path":"source"})).await;
    f.ok("fs.write", json!({"path":"source/item.txt","text":"kept"}))
        .await;
    let denied = f
        .call(
            Arc::new(Principal::owner()),
            "fs.copy",
            json!({"from":"source","to":"source/copy"}),
        )
        .await;
    assert_eq!(denied["error"]["code"], -32602);
    assert!(!f.root.path().join("source/copy").exists());
    f.ok("fs.copy", json!({"from":"source","to":"copied"}))
        .await;
    assert_eq!(
        fs::read_to_string(f.root.path().join("copied/item.txt")).unwrap(),
        "kept"
    );
}

#[tokio::test]
async fn test_workspace_set_reuses_existing_agent_workspace_owner() {
    let f = Fixture::new();
    fs::create_dir(f.root.path().join("next")).unwrap();
    let old = f.ok("workspace.get", json!({})).await;
    assert!(Path::new(old["path"].as_str().unwrap()).is_dir());
    let result = f.ok("workspace.set", json!({"path":"next"})).await;
    let actual = fs::canonicalize(f.root.path().join("next")).unwrap();
    assert_eq!(
        fs::canonicalize(result["path"].as_str().unwrap()).unwrap(),
        actual
    );
    assert_eq!(*f.ctx.state.current_workspace_dir.read().await, actual);
    assert_eq!(cluaiz_agents::ToolsEngine::workspace_dir(), actual);
    assert_eq!(f.ok("workspace.get", json!({})).await, result);
}

#[cfg(any(windows, unix))]
#[tokio::test]
async fn test_real_symlink_cannot_read_write_list_or_copy_outside_roots() {
    let f = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    fs::create_dir(f.root.path().join("source")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(outside.path(), f.root.path().join("source/link")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), f.root.path().join("source/link")).unwrap();
    let principal = f.principal(vec![Scope::Read, Scope::Write]);
    for (method, params) in [
        ("fs.read", json!({"path":"source/link/secret.txt"})),
        (
            "fs.write",
            json!({"path":"source/link/blocked.txt","text":"blocked"}),
        ),
        ("fs.list", json!({"path":"source","recursive":true})),
        ("fs.copy", json!({"from":"source","to":"destination"})),
    ] {
        let reply = f.call(principal.clone(), method, params).await;
        assert_eq!(reply["error"]["code"], -32002, "{method}: {reply}");
    }
    assert!(!outside.path().join("blocked.txt").exists());
    assert!(!f.root.path().join("destination/link/secret.txt").exists());
}

#[tokio::test]
async fn test_discovery_has_all_ten_methods_and_scope_metadata() {
    let f = Fixture::new();
    let document = f.ok("rpc.discover", json!({"group":"fs"})).await;
    assert_eq!(document["methods"].as_array().unwrap().len(), 8);
    let methods = document["methods"].as_array().unwrap();
    let delete = methods.iter().find(|m| m["name"] == "fs.delete").unwrap();
    assert_eq!(delete["x-cep"]["scope"], "write");
    assert_eq!(delete["x-cep"]["destructive"], true);
    let picker = methods
        .iter()
        .find(|m| m["name"] == "fs.pick_folder")
        .unwrap();
    assert_eq!(picker["x-cep"]["owner_only"], true);
    assert_eq!(picker["x-cep"]["mcp"], false);
    let workspace = f.ok("rpc.discover", json!({"group":"workspace"})).await;
    assert_eq!(workspace["methods"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_filesystem_results_match_old_golden_domain_fields() {
    let f = Fixture::new();
    let records: Vec<Value> = serde_json::from_str(include_str!("golden/fs.json")).unwrap();
    let listing = records
        .iter()
        .find(|r| r["route"] == "/v1/fs/list")
        .unwrap();
    // Create actual files with the recorded sizes; recorded read contents replace them below.
    for item in listing["body"]["items"].as_array().unwrap() {
        let path = f.root.path().join(item["path"].as_str().unwrap());
        assert!(path.starts_with(f.root.path()));
        if item["isFolder"] == true {
            fs::create_dir_all(path).unwrap();
        } else {
            fs::write(path, vec![b'x'; item["length"].as_u64().unwrap() as usize]).unwrap();
        }
    }
    for record in records
        .iter()
        .filter(|r| r["route"] == "/v1/fs/read" && r["status"] == 200)
    {
        use base64::prelude::*;
        let bytes = if record["body"]["is_binary"] == true {
            BASE64_STANDARD
                .decode(record["body"]["content"].as_str().unwrap())
                .unwrap()
        } else {
            record["body"]["content"]
                .as_str()
                .unwrap()
                .as_bytes()
                .to_vec()
        };
        fs::write(
            f.root
                .path()
                .join(record["request"]["body"]["path"].as_str().unwrap()),
            bytes,
        )
        .unwrap();
    }
    for record in records {
        let route = record["route"].as_str().unwrap();
        let mut params = record["request"]["body"].as_object().unwrap().clone();
        params.remove("caller");
        for (old, new) in [
            ("is_binary", "binary"),
            ("dir_path", "path"),
            ("root_path", "root"),
            ("old_path", "from"),
            ("new_path", "to"),
            ("src_path", "from"),
            ("dest_path", "to"),
        ] {
            if let Some(value) = params.remove(old) {
                params.insert(new.into(), value);
            }
        }
        if let Some(content) = params.remove("content") {
            let key = if params.remove("binary") == Some(json!(true)) {
                "data_base64"
            } else {
                "text"
            };
            params.insert(key.into(), content);
        }
        let method = format!("fs.{}", route.rsplit('/').next().unwrap());
        let reply = f
            .call(Arc::new(Principal::owner()), &method, Value::Object(params))
            .await;
        if record["status"] == 404 {
            assert_eq!(reply["error"]["code"], -32004, "{reply}");
            continue;
        }
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        if method == "fs.read" {
            let old = &record["body"];
            let new = &reply["result"];
            assert_eq!(new["binary"], old["is_binary"]);
            let key = if old["is_binary"] == true {
                "data_base64"
            } else {
                "text"
            };
            assert_eq!(new[key], old["content"]);
        } else if method == "fs.list" {
            fn items(list: &Value, old: bool) -> Vec<String> {
                let mut items: Vec<_> = list
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| {
                        json!([
                            v["path"],
                            v["name"],
                            v[if old { "isFolder" } else { "is_folder" }],
                            v[if old { "length" } else { "size" }]
                        ])
                        .to_string()
                    })
                    .collect();
                items.sort();
                items
            }
            assert_eq!(
                items(&reply["result"]["items"], false),
                items(&record["body"]["items"], true)
            );
        }
    }
}

#[tokio::test]
async fn test_approved_file_copy_does_not_consume_approval_twice() {
    let f = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let from = outside.path().join("approved.txt");
    fs::write(&from, "approved copy").unwrap();
    let mut principal = (*f.principal(vec![Scope::Read, Scope::Write])).clone();
    principal.on_denied = OnDenied::Ask;
    let principal = Arc::new(principal);
    let mut params = json!({"from":from,"to":"copy.txt"});
    let pending = f.call(principal.clone(), "fs.copy", params.clone()).await;
    assert_eq!(pending["error"]["code"], -32003);
    let approval = pending["error"]["data"]["approval_id"].as_str().unwrap();
    f.engine
        .shared()
        .approvals
        .decide(approval, true, None)
        .unwrap();
    params["_meta"] = json!({"approval":approval});
    let replay = f.call(principal, "fs.copy", params).await;
    assert!(
        replay.get("error").is_none(),
        "Approved copy asked again: {replay}"
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("copy.txt")).unwrap(),
        "approved copy"
    );
}
