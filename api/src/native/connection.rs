use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
    Json, Router,
};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::watch,
};
use tower::ServiceExt;

const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

pub(super) async fn serve<S>(
    stream: S,
    router: Router,
    token: Arc<String>,
    mut shutdown: watch::Receiver<bool>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service = service_fn(move |incoming: Request<hyper::body::Incoming>| {
        let router = router.clone();
        let token = token.clone();
        async move {
            let request = incoming.map(Body::new);
            // Mandatory native ingress gate, including public probes and auth-disabled profiles.
            let valid = crate::auth::extract_bearer_token(&request).is_some_and(|candidate| {
                !token.is_empty() && crate::auth::constant_time_compare(&candidate, &token)
            });
            if !valid {
                let mut response = (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                    "error": "Unauthorized", "message": "Native transport requires the Engine session Bearer token."
                }))).into_response();
                response
                    .headers_mut()
                    .insert("connection", axum::http::HeaderValue::from_static("close"));
                return Ok::<_, Infallible>(response);
            }
            router.oneshot(request).await
        }
    });
    let mut builder = http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_TIMEOUT)
        .max_buf_size(32 * 1024);
    let connection = builder.serve_connection(TokioIo::new(stream), service);
    tokio::pin!(connection);
    tokio::select! {
        result = &mut connection => {
            if let Err(error) = result { tracing::debug!("Native HTTP connection closed: {error}"); }
        }
        _ = shutdown.changed() => {
            connection.as_mut().graceful_shutdown();
            let _ = tokio::time::timeout(SHUTDOWN_GRACE, &mut connection).await;
        }
    }
}
