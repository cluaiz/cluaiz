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

/// `ingest.run` to its end: the task's result, or the error (at once, or the task's failure)
pub async fn ingest(router: &Router, params: Value) -> Result<std::result::Result<Value, Value>> {
    let task = match cep(router, "ingest.run", params).await? {
        Ok(task) => task,
        Err(error) => return Ok(Err(error)),
    };
    let id = task["task"]["task_id"].as_str().ok_or_else(|| anyhow::anyhow!("No task id: {task}"))?.to_string();
    // Model work can take minutes; tasks.result waits at most 30 s per call
    for _ in 0..40 {
        match cep(router, "tasks.result", serde_json::json!({ "task_id": id, "wait_ms": 30000 })).await? {
            Err(error) if error["data"]["type"] == "timeout" => continue,
            done => return Ok(done),
        }
    }
    anyhow::bail!("ingest task {id} did not finish in 20 minutes")
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
