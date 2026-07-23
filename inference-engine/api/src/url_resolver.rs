use eyre::{Result, eyre};
use std::path::Path;
use std::fs;
use base64::{Engine as _, engine::general_purpose::STANDARD as base64};
use reqwest;

/// Resolves a URL (http, data:base64, or file) into a local temporary file.
/// Returns the absolute path to the local file.
pub async fn resolve_to_local_file(url: &str) -> Result<String> {
    let temp_dir = cluaiz_shared::environment::EnvironmentManager::current().local_dir.join("temp_media");
    if !temp_dir.exists() {
        fs::create_dir_all(&temp_dir)?;
    }

    let file_id = uuid::Uuid::new_v4().to_string();
    let temp_file_path = temp_dir.join(&file_id);

    if url.starts_with("data:") {
        // Handle base64 data URI
        // Format: data:image/jpeg;base64,/9j/4AAQ...
        let parts: Vec<&str> = url.splitn(2, ',').collect();
        if parts.len() != 2 {
            return Err(eyre!("Invalid data URI format"));
        }
        
        let header = parts[0];
        let b64_data = parts[1];
        
        let ext = match header {
            h if h.contains("image/png") => "png",
            h if h.contains("image/jpeg") || h.contains("image/jpg") => "jpg",
            h if h.contains("image/webp") => "webp",
            h if h.contains("image/gif") => "gif",
            h if h.contains("image/bmp") => "bmp",
            h if h.contains("audio/mpeg") => "mp3",
            h if h.contains("audio/wav") => "wav",
            h if h.contains("audio/ogg") => "ogg",
            h if h.contains("audio/flac") => "flac",
            h if h.contains("video/mp4") => "mp4",
            h if h.contains("text/plain") => "txt",
            h if h.contains("text/markdown") => "md",
            h if h.contains("text/csv") => "csv",
            h if h.contains("application/pdf") => "pdf",
            h if h.contains("application/json") => "json",
            _ => "bin",
        };
                  
        let final_path = temp_file_path.with_extension(ext);
        
        let decoded = base64.decode(b64_data)?;
        fs::write(&final_path, decoded)?;
        
        Ok(final_path.to_string_lossy().to_string())

    } else if url.starts_with("http://") || url.starts_with("https://") {
        // Download from web
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            return Err(eyre!("Failed to download media from URL: HTTP {}", response.status()));
        }
        
        let content_type = response.headers().get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
            
        let ext = match content_type {
            c if c.contains("image/png") => "png",
            c if c.contains("image/jpeg") || c.contains("image/jpg") => "jpg",
            c if c.contains("image/webp") => "webp",
            c if c.contains("image/gif") => "gif",
            c if c.contains("image/bmp") => "bmp",
            c if c.contains("audio/mpeg") => "mp3",
            c if c.contains("audio/wav") => "wav",
            c if c.contains("audio/ogg") => "ogg",
            c if c.contains("audio/flac") => "flac",
            c if c.contains("video/mp4") => "mp4",
            c if c.contains("text/plain") => "txt",
            c if c.contains("text/markdown") => "md",
            c if c.contains("text/csv") => "csv",
            c if c.contains("application/pdf") => "pdf",
            c if c.contains("application/json") => "json",
            _ => "bin",
        };
                  
        let final_path = temp_file_path.with_extension(ext);
        
        let bytes = response.bytes().await?;
        fs::write(&final_path, bytes)?;
        
        Ok(final_path.to_string_lossy().to_string())

    } else if url.starts_with("file://") {
        // Already a local file
        let path_str = url.trim_start_matches("file://");
        let path = Path::new(path_str);
        if !path.exists() {
            return Err(eyre!("Local file not found: {}", path_str));
        }
        Ok(path.to_string_lossy().to_string())
    } else {
        // Assume it's a raw local path
        let path = Path::new(url);
        if !path.exists() {
            return Err(eyre!("Local path not found or unsupported protocol: {}", url));
        }
        Ok(path.to_string_lossy().to_string())
    }
}
