use super::*;
use std::io::{Read, Write};

fn serve(response: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        let mut used = 0;
        loop {
            assert!(
                used < request.len(),
                "HTTP fixture request headers too large"
            );
            stream.read_exact(&mut request[used..used + 1]).unwrap();
            used += 1;
            if request[..used].ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let _ = stream.write_all(response);
    });
    (url, task)
}

#[tokio::test]
async fn test_source_http_chunked_download_preserves_content_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let mut resolver = resolver(dir.path(), 5);
    resolver.client = Client::builder().no_proxy().build().unwrap();
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n3\r\nhel\r\n2\r\nlo\r\n0\r\n\r\n");
    let source = resolver.resolve(&url, |_| Ok(())).await.unwrap();
    server.join().unwrap();
    let path = source.path().to_path_buf();
    assert_eq!(source.read_bytes(resolver.limits).await.unwrap(), b"hello");
    drop(source);
    assert!(!path.exists());
}

#[tokio::test]
async fn test_source_http_excess_length_chunked_excess_and_bad_status_reject() {
    let dir = tempfile::tempdir().unwrap();
    let mut resolver = resolver(dir.path(), 4);
    resolver.client = Client::builder().no_proxy().build().unwrap();
    for response in [
        &b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"[..],
        &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n0\r\n\r\n"[..],
        &b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"[..],
    ] {
        let (url, server) = serve(response);
        assert!(resolver.resolve(&url, |_| Ok(())).await.is_err());
        server.join().unwrap();
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

fn resolver(dir: &Path, limit: u64) -> SourceResolver {
    let settings = MediaSettings {
        limits: MediaLimits::new(limit, limit).unwrap(),
        ..Default::default()
    };
    SourceResolver::new(dir.to_path_buf(), settings, FormatRegistry::default()).unwrap()
}

#[tokio::test]
async fn test_source_local_content_preserved_and_original_not_removed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.txt");
    std::fs::write(&path, b"hello").unwrap();
    let source = resolver(dir.path(), 5)
        .resolve(path.to_str().unwrap(), |_| Ok(()))
        .await
        .unwrap();
    assert_eq!(
        source
            .read_bytes(MediaLimits::new(5, 5).unwrap())
            .await
            .unwrap(),
        b"hello"
    );
    drop(source);
    assert!(path.exists());
}

#[tokio::test]
async fn test_source_data_uri_clone_keeps_asset_until_last_owner_drops() {
    let dir = tempfile::tempdir().unwrap();
    let source = resolver(dir.path(), 5)
        .resolve("data:text/plain;base64,aGVsbG8=", |_| Ok(()))
        .await
        .unwrap();
    let path = source.path().to_path_buf();
    let clone = source.clone();
    drop(source);
    assert!(path.exists());
    assert_eq!(
        clone
            .read_bytes(MediaLimits::new(5, 5).unwrap())
            .await
            .unwrap(),
        b"hello"
    );
    drop(clone);
    assert!(!path.exists());
}

#[tokio::test]
async fn test_source_oversize_bad_base64_and_denied_local_reject() {
    let dir = tempfile::tempdir().unwrap();
    let resolver = resolver(dir.path(), 4);
    assert!(resolver
        .resolve("data:text/plain;base64,aGVsbG8=", |_| Ok(()))
        .await
        .is_err());
    assert!(resolver
        .resolve("data:text/plain;base64,!!!!", |_| Ok(()))
        .await
        .is_err());
    assert!(resolver
        .resolve("data:text/plain,hello", |_| Ok(()))
        .await
        .is_err());
    assert!(resolver
        .resolve("ftp://invalid/media", |_| Ok(()))
        .await
        .is_err());
    let local = dir.path().join("input.txt");
    std::fs::write(&local, b"text").unwrap();
    assert!(resolver
        .resolve(local.to_str().unwrap(), |_| anyhow::bail!("Access denied"))
        .await
        .is_err());
    std::fs::write(&local, b"oversize").unwrap();
    assert!(resolver
        .resolve(local.to_str().unwrap(), |_| Ok(()))
        .await
        .is_err());
}

#[tokio::test]
async fn test_source_file_uri_and_growing_local_file_obey_limit() {
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("space name.txt");
    std::fs::write(&local, b"text").unwrap();
    let url = reqwest::Url::from_file_path(&local).unwrap();
    let source = resolver(dir.path(), 4)
        .resolve(url.as_str(), |_| Ok(()))
        .await
        .unwrap();
    std::fs::write(&local, b"larger").unwrap();
    assert!(source
        .read_bytes(MediaLimits::new(4, 4).unwrap())
        .await
        .is_err());
}
