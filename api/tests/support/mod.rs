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
