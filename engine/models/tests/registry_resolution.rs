//! Loopback HTTP acceptance for the anonymous registry client; no public API or weights.
use cluaiz_models::{
    parse_hf_repository, resolve_model_repo, resolve_model_repo_with_fallback, RegistryResolveError,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn fixture(
    status: u16,
    headers: &str,
    body: &str,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener");
    let address = listener.local_addr().expect("listener address");
    let response = format!(
        "HTTP/1.1 {status} Test\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
        body.len()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("one resolver request");
        let mut request = Vec::new();
        let mut chunk = [0u8; 1024];
        while !request.ends_with(b"\r\n\r\n") {
            let n = stream.read(&mut chunk).await.expect("request headers");
            assert!(n > 0 && request.len() < 8192);
            request.extend_from_slice(&chunk[..n]);
        }
        let _ = stream.write_all(response.as_bytes()).await;
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "resolver must not retry or send another event"
        );
        String::from_utf8(request).expect("ASCII request headers")
    });
    (format!("http://{address}/resolve"), server)
}

struct EndpointRestore(Option<std::ffi::OsString>);
impl Drop for EndpointRestore {
    fn drop(&mut self) {
        match &self.0 {
            Some(value) => std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", value),
            None => std::env::remove_var("CLUAIZ_RESOLVER_ENDPOINT"),
        }
    }
}

#[tokio::test]
async fn registry_client_sends_one_request_and_distinguishes_failures() {
    let _restore = EndpointRestore(std::env::var_os("CLUAIZ_RESOLVER_ENDPOINT"));
    let (endpoint, server) = fixture(200, "", r#"{"found":true,"hf_repo":"owner/model"}"#).await;
    std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", &endpoint);
    assert_eq!(
        resolve_model_repo_with_fallback("model:Q4", Some("model"))
            .await
            .expect("found"),
        Some("owner/model".into())
    );
    let sent = server.await.expect("server");
    assert!(sent.contains("id=model%3AQ4&fallback_id=model"));
    assert!(sent
        .to_ascii_lowercase()
        .contains("x-cluaiz-client: native-cli"));
    assert!(
        !sent.contains("x-cluaiz-origin"),
        "the edge secret must never be in the Engine"
    );

    let (endpoint, server) = fixture(200, "", r#"{"found":false}"#).await;
    std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", endpoint);
    assert_eq!(
        resolve_model_repo("missing").await.expect("valid miss"),
        None
    );
    server.await.expect("server");

    for (status, headers, expected) in [
        (
            429,
            "Retry-After: 7\r\n",
            RegistryResolveError::RateLimited(Some(7)),
        ),
        (503, "", RegistryResolveError::Unavailable(503)),
        (400, "", RegistryResolveError::InvalidInput),
        (413, "", RegistryResolveError::InvalidInput),
        (403, "", RegistryResolveError::ClientRejected(403)),
        (401, "", RegistryResolveError::ClientRejected(401)),
        (
            302,
            "Location: http://127.0.0.1/another\r\n",
            RegistryResolveError::Unavailable(302),
        ),
    ] {
        let (endpoint, server) = fixture(status, headers, "{}").await;
        std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", endpoint);
        assert_eq!(resolve_model_repo("model").await, Err(expected));
        server.await.expect("server");
    }
    for body in [
        "not-json",
        r#"{"success":false}"#,
        r#"{"found":true,"hf_repo":"owner/model?attack"}"#,
        r#"{"found":true,"hf_repo":"https://other.example/owner/model"}"#,
    ] {
        let (endpoint, server) = fixture(200, "", body).await;
        std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", endpoint);
        assert_eq!(
            resolve_model_repo("model").await,
            Err(RegistryResolveError::InvalidResponse)
        );
        server.await.expect("server");
    }
    let oversized = " ".repeat(64 * 1024 + 1);
    let (endpoint, server) = fixture(200, "", &oversized).await;
    std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", endpoint);
    assert_eq!(
        resolve_model_repo("model").await,
        Err(RegistryResolveError::InvalidResponse)
    );
    server.await.expect("server");

    // A complete repository does not contact even an unreachable registry endpoint.
    std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", "not-a-url");
    for input in [
        "owner/model",
        "hf://owner/model",
        "https://huggingface.co/owner/model/",
    ] {
        assert_eq!(
            resolve_model_repo(input).await.expect("direct HF"),
            Some("owner/model".into())
        );
    }
    assert_eq!(
        resolve_model_repo_with_fallback("model:Q4", Some("other")).await,
        Err(RegistryResolveError::InvalidInput)
    );

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener");
    let address = listener.local_addr().expect("address");
    drop(listener);
    std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", format!("http://{address}"));
    assert!(matches!(
        resolve_model_repo("model").await,
        Err(RegistryResolveError::Transport(_))
    ));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener");
    let address = listener.local_addr().expect("address");
    std::env::set_var("CLUAIZ_RESOLVER_ENDPOINT", format!("http://{address}"));
    let stalled = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("request");
        let mut buffer = [0u8; 2048];
        stream.read(&mut buffer).await.expect("headers");
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    assert!(matches!(
        resolve_model_repo("model").await,
        Err(RegistryResolveError::Transport(_))
    ));
    stalled.abort();
}

#[test]
fn hf_coordinates_reject_other_hosts_and_path_injection() {
    assert_eq!(
        parse_hf_repository("https://huggingface.co/owner/model/"),
        Some("owner/model".into())
    );
    for value in [
        "owner/../model",
        "owner/model?query",
        "owner/model#fragment",
        "owner//model",
        "https://other.example/owner/model",
        "hf://owner",
        "../model",
    ] {
        assert_eq!(parse_hf_repository(value), None, "{value}");
    }
}
