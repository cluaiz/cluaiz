//! `POST /v1/cep` through the Engine's real router: content type, notifications, discovery,
//! `system.info`, and the routes `system.info` replaced being gone. Only read methods are
//! called, so nothing is written to the selected home.

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
    for name in ["rpc.discover", "system.info", "tasks.get", "tasks.result", "tasks.cancel", "tasks.list"] {
        assert!(names.contains(&name), "{name} missing from {names:?}");
    }
}

#[tokio::test]
async fn test_routes_folded_into_system_info_are_gone() {
    for (method, path) in [("GET", "/info"), ("GET", "/hardware"), ("GET", "/v1/system/info"), ("GET", "/v1/system/control"), ("POST", "/v1/system/profile"), ("POST", "/v1/hardware/calibrate")] {
        let request = Request::builder().method(method).uri(path).body(Body::empty()).unwrap();
        assert_eq!(router().oneshot(request).await.unwrap().status(), StatusCode::NOT_FOUND, "{path}");
    }
}
