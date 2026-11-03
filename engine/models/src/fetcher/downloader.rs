//! ═══════════════════════════════════════════════════════════════════════
//!   Fetcher: Atomic & Resumable File Downloader
//! ═══════════════════════════════════════════════════════════════════════
//!
//! A file is written to `<name>.part` and renamed when every byte has arrived. A dropped
//! connection continues from the bytes already in `.part` with an HTTP `Range` request, a few
//! times in a row; a later run of the same download continues too. A server that ignores `Range`
//! sends the whole file again, and `.part` starts over.

use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Progress(f32, u64, u64, f64, u64),
    Complete(String),
    Error(String, String),
    PurgeComplete(String),
    PurgeError(String, String),
}

/// Tries per download run before it gives up (the bytes in `.part` stay for the next run)
const ATTEMPTS: u32 = 6;
/// Longest wait between two tries
const MAX_BACKOFF_SECS: u64 = 30;
/// The error text of a download the user stopped
pub const ABORTED: &str = "Download aborted by user.";

/// Why one try ended
enum Failure {
    /// Try again: the connection dropped, the server was busy, or the bytes did not add up
    Retry(String),
    /// Stop: the source refused the file, the disk failed, or the user stopped it
    Fatal(String),
}

pub struct FileDownloader;

impl FileDownloader {
    /// Downloads one URL to `dest_path` through `<dest_path>.part`, continuing from what an
    /// interrupted try or an earlier run left in `.part`
    pub async fn download_single_file(
        client: &Client,
        url: &str,
        dest_path: &Path,
        tx: mpsc::Sender<DownloadEvent>,
        abort: Arc<AtomicBool>,
    ) -> Result<(), String> {
        if dest_path.exists() {
            let _ = tx.send(DownloadEvent::Complete(dest_path.to_string_lossy().to_string())).await;
            return Ok(());
        }
        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }
        let part_path = part_path(dest_path);
        let mut last_error = String::new();
        for attempt in 1..=ATTEMPTS {
            if attempt > 1 {
                let wait = (1u64 << (attempt - 1)).min(MAX_BACKOFF_SECS);
                let have = file_len(&part_path).await;
                eprintln!(
                    "   │  {last_error}; continuing from {:.1} MB in {wait} s (try {attempt}/{ATTEMPTS})",
                    have as f64 / 1_048_576.0
                );
                tracing::warn!(url, error = %last_error, have, attempt, "download interrupted; resuming");
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                if abort.load(Ordering::Relaxed) {
                    let _ = tokio::fs::remove_file(&part_path).await;
                    return Err(ABORTED.to_string());
                }
            }
            match Self::attempt(client, url, &part_path, &tx, &abort).await {
                Ok(()) => {
                    tokio::fs::rename(&part_path, dest_path)
                        .await
                        .map_err(|e| format!("Failed to finalize downloaded file: {}", e))?;
                    let _ = tx.try_send(DownloadEvent::Complete(dest_path.to_string_lossy().to_string()));
                    return Ok(());
                }
                Err(Failure::Fatal(error)) => {
                    let _ = tx
                        .send(DownloadEvent::Error(dest_path.to_string_lossy().to_string(), error.clone()))
                        .await;
                    return Err(error);
                }
                Err(Failure::Retry(error)) => last_error = error,
            }
        }
        let have = file_len(&part_path).await;
        let error = format!(
            "{last_error} after {ATTEMPTS} tries; {:.1} MB is kept, and running the same download again continues from there",
            have as f64 / 1_048_576.0
        );
        let _ = tx
            .send(DownloadEvent::Error(dest_path.to_string_lossy().to_string(), error.clone()))
            .await;
        Err(error)
    }

    /// One request: from the start, or from the end of `.part`
    async fn attempt(
        client: &Client,
        url: &str,
        part_path: &Path,
        tx: &mpsc::Sender<DownloadEvent>,
        abort: &AtomicBool,
    ) -> Result<(), Failure> {
        let offset = file_len(part_path).await;
        let mut req = client.get(url);
        if let Some(token) = crate::models::fetcher::hf_hub::HuggingFaceHub::resolve_hf_token() {
            req = req.header(header::AUTHORIZATION, format!("Bearer {}", token));
        }
        if offset > 0 {
            req = req.header(header::RANGE, format!("bytes={offset}-"));
        }
        let response = req
            .send()
            .await
            .map_err(|e| Failure::Retry(format!("Failed to connect to source {}: {}", url, e)))?;
        let status = response.status();
        let range = content_range(&response);

        if status == StatusCode::RANGE_NOT_SATISFIABLE && offset > 0 {
            // `.part` may already hold the whole file (the rename did not happen)
            if range.and_then(|(_, total)| total) == Some(offset) {
                return Ok(());
            }
            let _ = tokio::fs::remove_file(part_path).await;
            return Err(Failure::Retry("the partial file does not match the source; starting again".into()));
        }
        if !status.is_success() {
            return Err(if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                Failure::Fatal(format!(
                    "🔒 [Gated Model Download Blocked - HTTP {}]\nThis Hugging Face repository requires an authorized token.\n1. Accept license terms on Hugging Face.\n2. Set your token:\n   PowerShell:   $env:HF_TOKEN=\"hf_your_token\"\n   Windows CMD:  set HF_TOKEN=hf_your_token\n   Linux / Mac:  export HF_TOKEN=\"hf_your_token\"",
                    status
                ))
            } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                Failure::Retry(format!("Download failed: HTTP {}", status))
            } else {
                Failure::Fatal(format!("Download failed: HTTP {}", status))
            });
        }

        // 206 continues `.part`; 200 is the whole file again
        let (start, total) = if status == StatusCode::PARTIAL_CONTENT && offset > 0 {
            match range {
                Some((first, total)) if first == offset => {
                    (offset, total.or_else(|| response.content_length().map(|len| offset + len)))
                }
                _ => {
                    let _ = tokio::fs::remove_file(part_path).await;
                    return Err(Failure::Retry("the server sent a different part of the file; starting again".into()));
                }
            }
        } else {
            (0, response.content_length())
        };
        let mut file = if start > 0 {
            tokio::fs::OpenOptions::new().append(true).open(part_path).await
        } else {
            tokio::fs::File::create(part_path).await
        }
        .map_err(|e| Failure::Fatal(e.to_string()))?;

        let total_size = total.unwrap_or(0);
        let pb = (total_size > 0).then(|| {
            let p = indicatif::ProgressBar::new(total_size);
            p.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template("   [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})")
                    .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar())
                    .progress_chars("━╾─"),
            );
            p.set_position(start);
            p.reset_eta();
            p
        });

        let mut stream = response.bytes_stream();
        let mut downloaded = start;
        let start_time = std::time::Instant::now();
        let mut last_flush = std::time::Instant::now();
        while let Some(item) = stream.next().await {
            if abort.load(Ordering::Relaxed) {
                if let Some(ref p) = pb {
                    p.abandon();
                }
                drop(file);
                let _ = tokio::fs::remove_file(part_path).await;
                return Err(Failure::Fatal(ABORTED.to_string()));
            }
            let chunk = match item {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = file.flush().await;
                    if let Some(ref p) = pb {
                        p.abandon();
                    }
                    return Err(Failure::Retry(format!("connection dropped: {error}")));
                }
            };
            file.write_all(&chunk).await.map_err(|e| Failure::Fatal(e.to_string()))?;
            downloaded += chunk.len() as u64;

            // Flush every second so Windows Explorer shows the size as it grows
            if last_flush.elapsed().as_secs_f32() >= 1.0 {
                let _ = file.flush().await;
                last_flush = std::time::Instant::now();
            }
            if let Some(ref p) = pb {
                p.set_position(downloaded);
            }
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                ((downloaded - start) as f64 / 1024.0 / 1024.0) / elapsed
            } else {
                0.0
            };
            let percent = if total_size > 0 {
                (downloaded as f32 / total_size as f32) * 100.0
            } else {
                0.0
            };
            let _ = tx.try_send(DownloadEvent::Progress(percent, downloaded, total_size, speed, 0));
        }
        file.flush().await.map_err(|e| Failure::Fatal(e.to_string()))?;
        drop(file);
        if let Some(ref p) = pb {
            p.finish_and_clear();
        }
        // A stream can end early without an error: only a complete file is renamed
        if let Some(total) = total {
            if downloaded < total {
                return Err(Failure::Retry(format!(
                    "the connection closed at {:.1} of {:.1} MB",
                    downloaded as f64 / 1_048_576.0,
                    total as f64 / 1_048_576.0
                )));
            }
            if downloaded > total {
                let _ = tokio::fs::remove_file(part_path).await;
                return Err(Failure::Retry("more bytes arrived than the file has; starting again".into()));
            }
        }
        Ok(())
    }
}

/// `<file>.part` beside the file (the full name is kept, so `a.gguf` and `a.onnx` do not share one)
pub fn part_path(dest_path: &Path) -> PathBuf {
    let mut name = dest_path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest_path.with_file_name(name)
}

async fn file_len(path: &Path) -> u64 {
    tokio::fs::metadata(path).await.map(|m| m.len()).unwrap_or(0)
}

/// `Content-Range: bytes <first>-<last>/<total|*>` as (first, total)
fn content_range(response: &reqwest::Response) -> Option<(u64, Option<u64>)> {
    parse_content_range(response.headers().get(header::CONTENT_RANGE)?.to_str().ok()?)
}

fn parse_content_range(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim();
    let (span, total) = rest.split_once('/')?;
    let total = total.trim().parse::<u64>().ok();
    let first = if span.trim() == "*" { 0 } else { span.split_once('-')?.0.trim().parse::<u64>().ok()? };
    Some((first, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_range_and_part_name() {
        assert_eq!(parse_content_range("bytes 100-199/200"), Some((100, Some(200))));
        assert_eq!(parse_content_range("bytes 5-9/*"), Some((5, None)));
        assert_eq!(parse_content_range("bytes */300"), Some((0, Some(300))));
        assert_eq!(parse_content_range("items 1-2/3"), None);
        assert_eq!(
            part_path(Path::new("dir/model.Q4.gguf")),
            Path::new("dir/model.Q4.gguf.part")
        );
    }

    /// A local server that drops the first connection halfway; `honour_range` decides whether the
    /// second answer continues (206) or sends the whole file again (200). Returns the file and the
    /// Range header of each request.
    async fn dropping_server(
        body: Vec<u8>,
        honour_range: bool,
    ) -> (String, Arc<std::sync::Mutex<Vec<Option<String>>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/model.gguf", listener.local_addr().unwrap());
        let ranges = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = Arc::clone(&ranges);
        tokio::spawn(async move {
            let mut served = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                }
                let text = String::from_utf8_lossy(&request).to_ascii_lowercase();
                let range = text
                    .lines()
                    .find_map(|l| l.strip_prefix("range: bytes="))
                    .map(|v| v.trim().trim_end_matches('-').to_string());
                seen.lock().unwrap().push(range.clone());
                served += 1;
                let len = body.len();
                if served == 1 {
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\n\r\n");
                    socket.write_all(head.as_bytes()).await.unwrap();
                    socket.write_all(&body[..len / 2]).await.unwrap();
                    continue; // dropped: the socket closes halfway
                }
                match range.and_then(|r| r.parse::<usize>().ok()).filter(|_| honour_range) {
                    Some(from) => {
                        let head = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{len}\r\n\r\n",
                            len - from,
                            len - 1
                        );
                        socket.write_all(head.as_bytes()).await.unwrap();
                        socket.write_all(&body[from..]).await.unwrap();
                    }
                    None => {
                        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\n\r\n");
                        socket.write_all(head.as_bytes()).await.unwrap();
                        socket.write_all(&body).await.unwrap();
                    }
                }
            }
        });
        (url, ranges)
    }

    #[tokio::test]
    async fn test_dropped_connection_continues_from_part_or_starts_over() {
        let body: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        for honour_range in [true, false] {
            let (url, ranges) = dropping_server(body.clone(), honour_range).await;
            let dir = tempfile::tempdir().unwrap();
            let dest = dir.path().join("model.gguf");
            let (tx, _rx) = mpsc::channel(1024);
            FileDownloader::download_single_file(&Client::new(), &url, &dest, tx, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
            assert_eq!(std::fs::read(&dest).unwrap(), body, "honour_range={honour_range}");
            assert!(!part_path(&dest).exists());
            let ranges = ranges.lock().unwrap().clone();
            assert_eq!(ranges.len(), 2);
            assert_eq!(ranges[0], None);
            // The second request asked for the rest, after the bytes that had arrived
            let from: u64 = ranges[1].as_deref().unwrap().parse().unwrap();
            assert!(from > 0 && from <= body.len() as u64 / 2, "{from}");
        }
    }
}
