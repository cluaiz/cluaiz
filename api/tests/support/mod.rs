use anyhow::Result;
use axum::{http::StatusCode, Router};
use serde_json::Value;
pub async fn post(
    router: &Router,
    route: &str,
    body: Value,
    caller: &str,
) -> Result<(StatusCode, String)> {
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let app = router.clone();
    let _server = Server(tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    }));
    let response = reqwest::Client::new()
        .post(format!("http://{address}{route}"))
        .bearer_auth("test_session_token")
        .header("x-caller", caller)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&body)?)
        .send()
        .await?;
    let status = response.status();
    Ok((status, response.text().await?))
}

/// One CEP call through `POST /v1/cep`: the reply's `result`, or its `error` object
pub async fn cep(router: &Router, method: &str, params: Value) -> Result<std::result::Result<Value, Value>> {
    let request = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let (status, body) = post(router, "/v1/cep", request, "user").await?;
    anyhow::ensure!(status == StatusCode::OK, "CEP HTTP {status}: {body}");
    let reply: Value = serde_json::from_str(&body)?;
    Ok(match reply.get("error") {
        Some(error) => Err(error.clone()),
        None => Ok(reply["result"].clone()),
    })
}
