//! Real OS endpoint acceptance; synthetic test routes exercise framing, not model behavior.
#![cfg(any(windows, unix))]
use anyhow::Result;
use axum::{
    body::{Body, Bytes},
    http::{Request, StatusCode},
    response::Sse,
    routing::{get, post},
    Router,
};
use cluaiz_api::{native::NativeListener, AppState};
use hyper_util::rt::TokioIo;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
};

const TOKEN: &str = "test_session_token";

#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type Stream = tokio::net::UnixStream;

async fn connect(endpoint: &str) -> Result<Stream> {
    #[cfg(windows)]
    {
        for _ in 0..100 {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint) {
                Ok(stream) => return Ok(stream),
                Err(error) if error.raw_os_error() == Some(231) => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                Err(error) => return Err(error.into()),
            }
        }
        anyhow::bail!("Native test pipe remained busy")
    }
    #[cfg(unix)]
    {
        Ok(tokio::net::UnixStream::connect(endpoint).await?)
    }
}

struct Harness {
    endpoint: String,
    stop: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Result<()>>>,
    _directory: tempfile::TempDir,
}
impl Harness {
    async fn start(router: Router) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        #[cfg(windows)]
        let endpoint = format!(r"\\.\pipe\cluaiz_test_{}", uuid::Uuid::new_v4().simple());
        #[cfg(unix)]
        let endpoint = {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
            directory
                .path()
                .join("engine.sock")
                .to_string_lossy()
                .into_owned()
        };
        let listener = NativeListener::bind(&endpoint)?;
        assert!(
            NativeListener::bind(&endpoint).is_err(),
            "Existing endpoint must not be replaced"
        );
        let (stop, receiver) = oneshot::channel();
        let task = tokio::spawn(listener.serve(router, Arc::new(TOKEN.into()), async {
            let _ = receiver.await;
        }));
        Ok(Self {
            endpoint,
            stop: Some(stop),
            task: Some(task),
            _directory: directory,
        })
    }
    async fn finish(mut self) -> Result<()> {
        let _ = self.stop.take().unwrap().send(());
        tokio::time::timeout(Duration::from_secs(4), self.task.take().unwrap()).await???;
        #[cfg(unix)]
        assert!(!std::path::Path::new(&self.endpoint).exists());
        Ok(())
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

struct Client {
    sender: hyper::client::conn::http1::SendRequest<Body>,
    task: tokio::task::JoinHandle<()>,
}
impl Client {
    async fn open(endpoint: &str) -> Result<Self> {
        let (sender, connection) =
            hyper::client::conn::http1::handshake(TokioIo::new(connect(endpoint).await?)).await?;
        Ok(Self {
            sender,
            task: tokio::spawn(async {
                let _ = connection.await;
            }),
        })
    }
    async fn request(
        &mut self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Vec<u8>,
    ) -> Result<(StatusCode, Vec<u8>)> {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "cluaiz.native");
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        let response = self
            .sender
            .send_request(builder.body(Body::from(body))?)
            .await?;
        let status = response.status();
        let bytes = axum::body::to_bytes(Body::new(response.into_body()), 256 * 1024).await?;
        Ok((status, bytes.to_vec()))
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn real_router(root: &std::path::Path) -> Router {
    let state = AppState::new_test_state(root.to_path_buf());
    // Native ingress still requires authentication when ordinary HTTP auth is disabled.
    state
        .permission_schema
        .try_write()
        .unwrap()
        .api_auth
        .required = false;
    cluaiz_api::create_router(state)
}

#[tokio::test]
async fn native_requires_session_auth_before_public_or_auth_disabled_routes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let server = Harness::start(real_router(root.path())).await?;
    for path in ["/health", "/mcp", "/v1/fs/read"] {
        for token in [None, Some("wrong-token")] {
            let mut client = Client::open(&server.endpoint).await?;
            assert_eq!(
                client.request("GET", path, token, vec![]).await?.0,
                StatusCode::UNAUTHORIZED
            );
        }
    }
    let mut client = Client::open(&server.endpoint).await?;
    assert_eq!(
        client
            .request("GET", "/health", Some(TOKEN), vec![])
            .await?
            .0,
        StatusCode::OK
    );
    assert_eq!(
        client
            .request("GET", "/missing-native-route", Some(TOKEN), vec![])
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    drop(client);
    server.finish().await
}

#[tokio::test]
async fn native_preserves_binary_sse_body_limits_and_partial_http_reads() -> Result<()> {
    let root = tempfile::tempdir()?;
    let router = real_router(root.path())
        .route(
            "/native-test/echo",
            post(|bytes: Bytes| async move { bytes }),
        )
        .route(
            "/native-test/events",
            get(|| async {
                Sse::new(futures::stream::iter([
                    Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default().data("first"),
                    ),
                    Ok(axum::response::sse::Event::default().data("second")),
                ]))
            }),
        );
    let server = Harness::start(router).await?;
    let mut client = Client::open(&server.endpoint).await?;
    let binary: Vec<u8> = (0..=255).cycle().take(48 * 1024).collect();
    let (status, result) = client
        .request("POST", "/native-test/echo", Some(TOKEN), binary.clone())
        .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, binary);
    let (status, result) = client
        .request("GET", "/native-test/events", Some(TOKEN), vec![])
        .await?;
    assert_eq!(status, StatusCode::OK);
    let text = String::from_utf8(result)?;
    assert!(
        text.contains("data: first") && text.contains("data: second"),
        "{text}"
    );
    let (status, _) = client
        .request(
            "POST",
            "/native-test/echo",
            Some(TOKEN),
            vec![0; 2 * 1024 * 1024 + 1],
        )
        .await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    drop(client);
    let mut raw = connect(&server.endpoint).await?;
    let request = format!("POST /native-test/echo HTTP/1.1\r\nHost: cluaiz.native\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
    for chunk in request.as_bytes().chunks(3) {
        raw.write_all(chunk).await?;
        tokio::task::yield_now().await;
    }
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), raw.read_to_end(&mut response)).await??;
    let text = String::from_utf8(response)?;
    assert!(text.starts_with("HTTP/1.1 200"), "{text}");
    assert!(text.ends_with("hello"), "{text}");
    server.finish().await
}

#[tokio::test]
async fn native_disconnect_drops_stream_and_shutdown_reaps_idle_connections() -> Result<()> {
    struct Dropped(Arc<AtomicUsize>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(AtomicUsize::new(0));
    let root = tempfile::tempdir()?;
    let observer = dropped.clone();
    let router = real_router(root.path()).route(
        "/native-test/hold",
        get(move || {
            let guard = Dropped(observer.clone());
            async move {
                axum::response::Response::new(Body::from_stream(async_stream::stream! {
                    let _guard = guard;
                    yield Ok::<_, std::io::Error>(Bytes::from_static(b"first"));
                    std::future::pending::<()>().await;
                }))
            }
        }),
    );
    let server = Harness::start(router).await?;
    let mut raw = connect(&server.endpoint).await?;
    raw.write_all(format!("GET /native-test/hold HTTP/1.1\r\nHost: cluaiz.native\r\nAuthorization: Bearer {TOKEN}\r\n\r\n").as_bytes()).await?;
    let mut bytes = [0u8; 256];
    assert!(raw.read(&mut bytes).await? > 0);
    drop(raw);
    tokio::time::timeout(Duration::from_secs(2), async {
        while dropped.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let _idle = connect(&server.endpoint).await?;
    server.finish().await
}

#[tokio::test]
async fn native_connection_capacity_recovers_when_an_owned_stream_disconnects() -> Result<()> {
    let root = tempfile::tempdir()?;
    let active = Arc::new(AtomicUsize::new(0));
    let observed = active.clone();
    let router = real_router(root.path()).route(
        "/native-test/capacity",
        get(move || {
            let active = observed.clone();
            async move {
                active.fetch_add(1, Ordering::SeqCst);
                axum::response::Response::new(Body::from_stream(async_stream::stream! {
                    yield Ok::<_, std::io::Error>(Bytes::from_static(b"running"));
                    std::future::pending::<()>().await;
                }))
            }
        }),
    );
    let server = Harness::start(router).await?;
    let mut streams = Vec::new();
    for _ in 0..cluaiz_api::native::MAX_NATIVE_CONNECTIONS {
        let mut stream = connect(&server.endpoint).await?;
        stream.write_all(format!("GET /native-test/capacity HTTP/1.1\r\nHost: cluaiz.native\r\nAuthorization: Bearer {TOKEN}\r\n\r\n").as_bytes()).await?;
        let mut bytes = [0u8; 256];
        assert!(tokio::time::timeout(Duration::from_secs(2), stream.read(&mut bytes)).await?? > 0);
        streams.push(stream);
    }
    assert_eq!(
        active.load(Ordering::SeqCst),
        cluaiz_api::native::MAX_NATIVE_CONNECTIONS
    );
    let mut pending = connect(&server.endpoint).await?;
    pending.write_all(format!("GET /health HTTP/1.1\r\nHost: cluaiz.native\r\nAuthorization: Bearer {TOKEN}\r\nConnection: close\r\n\r\n").as_bytes()).await?;
    let mut bytes = [0u8; 512];
    assert!(
        tokio::time::timeout(Duration::from_millis(50), pending.read(&mut bytes))
            .await
            .is_err()
    );
    drop(streams.remove(0));
    let count = tokio::time::timeout(Duration::from_secs(2), pending.read(&mut bytes)).await??;
    assert!(String::from_utf8_lossy(&bytes[..count]).starts_with("HTTP/1.1 200"));
    drop(streams);
    drop(pending);
    server.finish().await
}

#[cfg(unix)]
#[tokio::test]
async fn unix_refuses_existing_paths_and_nonprivate_parent() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir()?;
    let path = root.path().join("keep");
    std::fs::write(&path, "keep")?;
    assert!(NativeListener::bind(path.to_str().unwrap()).is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "keep");
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755))?;
    assert!(NativeListener::bind(root.path().join("private.sock").to_str().unwrap()).is_err());
    Ok(())
}
