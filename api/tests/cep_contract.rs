//! `POST /v1/cep` through the Engine's real router: content type, notifications, discovery,
//! `system.info`, and the route table. Only read methods are called, so nothing is written to the
//! selected home.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cluaiz_api::{create_router, AppState};
use serde_json::{json, Value};
use tower::ServiceExt;

fn router() -> axum::Router {
    create_router(AppState::new_test_state(std::env::temp_dir()))
}

async fn post(path: &str, content_type: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder().method("POST").uri(path).header("content-type", content_type).body(Body::from(body.to_string())).unwrap();
    let response = router().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn test_cep_endpoint_basics() {
    let (status, reply) = post("/v1/cep", "application/json", json!({"jsonrpc": "2.0", "id": 1, "method": "system.info"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["result"]["engine"], "Cluaiz Engine");
    assert_eq!(reply["result"]["cep_version"], 1);
    assert!(reply["result"]["hardware"].is_object());

    let (status, reply) = post("/v1/cep", "text/plain", json!({"jsonrpc": "2.0", "id": 1, "method": "system.info"})).await;
    assert_eq!((status, reply["error"]["code"].as_i64()), (StatusCode::UNSUPPORTED_MEDIA_TYPE, Some(-32600)));

    let (status, _) = post("/v1/cep", "application/json", json!({"jsonrpc": "2.0", "method": "system.info"})).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (_, reply) = post("/v1/cep", "application/json", json!({"jsonrpc": "2.0", "id": 2, "method": "rpc.discover"})).await;
    let names: Vec<&str> = reply["result"]["methods"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str()).collect();
    assert!(!names.contains(&"ingest.run"));
    let (_, missing) = post("/v1/cep", "application/json", json!({"jsonrpc":"2.0","id":3,"method":"ingest.run","params":{}})).await;
    assert_eq!(missing["error"]["code"], -32601);
    for name in ["rpc.discover", "system.info", "tasks.get", "tasks.result", "tasks.cancel", "tasks.list"] {
        assert!(names.contains(&name), "{name} missing from {names:?}");
    }
}

/// `/mcp`: 2025 `initialize` unchanged, `server/discover`, stateless 2026-07-28 requests and -32022
#[tokio::test]
async fn test_mcp_discover_stateless_and_unsupported_version() {
    let (_, init) = post("/mcp", "application/json", json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    let (_, odd) = post("/mcp", "application/json", json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}})).await;
    assert_eq!(odd["result"]["protocolVersion"], "2025-11-25");

    let (_, found) = post("/mcp", "application/json", json!({"jsonrpc":"2.0","id":2,"method":"server/discover"})).await;
    let versions: Vec<&str> = found["result"]["supportedVersions"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(versions.contains(&"2026-07-28") && versions.contains(&"2025-11-25"), "{versions:?}");
    assert_eq!(found["result"]["serverInfo"]["name"], "cluaiz-engine");
    assert_eq!(found["result"]["capabilities"], init["result"]["capabilities"]);

    let meta = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28"});
    let (_, listed) = post("/mcp", "application/json", json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"_meta": meta}})).await;
    assert!(listed["result"]["tools"].is_array(), "{listed}");

    let bad = json!({"io.modelcontextprotocol/protocolVersion": "2031-01-01"});
    let (_, refused) = post("/mcp", "application/json", json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{"_meta": bad}})).await;
    assert_eq!(refused["error"]["code"], -32022);
    assert!(refused["error"]["data"]["supported"].as_array().unwrap().iter().any(|v| v == "2026-07-28"));

    // A 2025 client without any version stays accepted
    let (_, plain) = post("/mcp", "application/json", json!({"jsonrpc":"2.0","id":5,"method":"tools/list"})).await;
    assert!(plain["result"]["tools"].is_array());
}

/// The route table is the market-standard routes plus CEP; nothing else exists
#[test]
fn test_route_table_is_the_standard_routes_and_cep() {
    let source = include_str!("../src/routes.rs");
    let mut routes: Vec<&str> = source
        .split(".route(")
        .skip(1)
        .filter_map(|rest| rest.split('"').nth(1))
        .collect();
    routes.sort_unstable();
    let mut expected = vec![
        "/health", "/v1/cep", "/mcp",
        "/v1/chat/completions", "/api/chat", "/v1/ingest", "/v1/embeddings", "/v1/audio/speech", "/v1/audio/transcriptions",
        "/v1/models", "/api/tags", "/api/pull", "/api/delete",
        "/api/generate", "/api/embed", "/api/version", "/api/ps", "/api/show",
    ];
    expected.sort_unstable();
    assert_eq!(routes, expected);
}
