//! PDFium, downloaded the first time a PDF page has to be rendered. It is not in the core package
//! and not in the Cluaiz release: the Engine fetches the official prebuilt from
//! `bblanchon/pdfium-binaries`, checks the pinned byte size and SHA-256 of the archive, extracts the
//! one library into `<CLUAIZ_HOME>/bin`, and writes a stamp naming the pinned release. The archive
//! check copies `cudnn_driver::verify_archive_file` (engine/onnx).

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

/// One pinned prebuilt archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    pub platform: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    /// The library's path inside the archive
    pub member: &'static str,
    /// The library's file name in `<CLUAIZ_HOME>/bin`
    pub library: &'static str,
}

/// The release every platform uses (published 2026-11-23, GitHub release `chromium/8179`).
pub const RELEASE: &str = "chromium/8179";

/// Byte sizes and digests are the GitHub release asset values of chromium/8179, read 2026-11-27.
pub const PINNED: [Pinned; 4] = [
    Pinned {
        platform: "win-x64",
        url: "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8179/pdfium-win-x64.tgz",
        bytes: 4_046_589,
        sha256: "836a77c9053f624839c66010cddea7b675628a36b8831d2330178bb169d241b6",
        member: "bin/pdfium.dll",
        library: "pdfium.dll",
    },
    Pinned {
        platform: "linux-x64",
        url: "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8179/pdfium-linux-x64.tgz",
        bytes: 3_953_118,
        sha256: "b4766a6bf2856efbcc6c3cc7d6ae9022bdc6bb36ddc1acdd92d1f1873f773f4b",
        member: "lib/libpdfium.so",
        library: "libpdfium.so",
    },
    Pinned {
        platform: "linux-arm64",
        url: "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8179/pdfium-linux-arm64.tgz",
        bytes: 3_819_293,
        sha256: "18bb6d568257b28d14f6c3b8bfb48caf73d4615b319ca4d43b424c2a03431d9f",
        member: "lib/libpdfium.so",
        library: "libpdfium.so",
    },
    Pinned {
        platform: "mac-arm64",
        url: "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8179/pdfium-mac-arm64.tgz",
        bytes: 3_643_140,
        sha256: "44b7c6f55dd33454d0909728bd0adf6c86a8e141b36f07a6b6f50e228e1dfb46",
        member: "lib/libpdfium.dylib",
        library: "libpdfium.dylib",
    },
];

/// The pinned archive for an OS and CPU (`std::env::consts` names), None where none is pinned.
pub fn pinned_for(os: &str, arch: &str) -> Option<&'static Pinned> {
    let platform = match (os, arch) {
        ("windows", "x86_64") => "win-x64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "aarch64") => "mac-arm64",
        _ => return None,
    };
    PINNED.iter().find(|pinned| pinned.platform == platform)
}

/// What the stamp file holds for a pinned archive: the release and the archive digest.
fn stamp_text(pinned: &Pinned) -> String {
    format!("{RELEASE} {} {}\n", pinned.platform, pinned.sha256)
}

fn stamp_path(bin: &Path) -> PathBuf {
    bin.join("pdfium.version")
}

/// Whether the library in `bin` is the pinned one, or a download is needed.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Ready(PathBuf),
    Download,
}

/// Ready only when the library file is there, not empty, and the stamp names this pinned archive.
/// A missing or different stamp (an older release, another platform, a half-finished earlier
/// download) means a fresh download.
pub fn decide(bin: &Path, pinned: &Pinned) -> Decision {
    let library = bin.join(pinned.library);
    let present = std::fs::metadata(&library).is_ok_and(|m| m.is_file() && m.len() > 0);
    let stamped = std::fs::read_to_string(stamp_path(bin)).is_ok_and(|s| s == stamp_text(pinned));
    if present && stamped {
        Decision::Ready(library)
    } else {
        Decision::Download
    }
}

/// Byte size and SHA-256 must both match (same check as `cudnn_driver::verify_archive_file`).
pub fn verify_archive_file(path: &Path, expected_bytes: u64, expected_sha256: &str) -> Result<()> {
    let meta = std::fs::metadata(path).with_context(|| format!("failed to stat archive {path:?}"))?;
    if meta.len() != expected_bytes {
        bail!("archive byte size mismatch: expected {expected_bytes}, got {}", meta.len());
    }
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        bail!("SHA-256 mismatch for {path:?}: expected {expected_sha256}, got {actual}");
    }
    Ok(())
}

/// One download at a time; a second request waits and then finds the library ready.
static FETCH: Mutex<()> = Mutex::new(());

/// The path of the pinned PDFium library in `<CLUAIZ_HOME>/bin`, downloading it first if needed.
pub fn ensure() -> Result<PathBuf> {
    let pinned = pinned_for(std::env::consts::OS, std::env::consts::ARCH).with_context(|| {
        format!(
            "No PDF renderer build is available for {}-{}, so pages without text cannot be rendered",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let bin = cluaiz_core::EnvironmentManager::current().bin_dir();
    ensure_in(&bin, pinned)
}

pub fn ensure_in(bin: &Path, pinned: &'static Pinned) -> Result<PathBuf> {
    let _lock = FETCH.lock().map_err(|_| anyhow::anyhow!("PDF renderer download lock is poisoned"))?;
    if let Decision::Ready(path) = decide(bin, pinned) {
        return Ok(path);
    }
    tracing::info!(release = RELEASE, platform = pinned.platform, bytes = pinned.bytes, "Downloading the PDF renderer (PDFium)");
    let bin_owned = bin.to_path_buf();
    // A plain thread: the blocking HTTP client must not run on an async runtime thread
    let result = std::thread::spawn(move || install(&bin_owned, pinned))
        .join()
        .map_err(|_| anyhow::anyhow!("PDF renderer download thread panicked"))?;
    result.with_context(|| {
        format!(
            "Downloading the PDF renderer (PDFium {RELEASE}, {:.1} MB from {}) failed; pages without text cannot be read until it succeeds",
            pinned.bytes as f64 / 1_000_000.0,
            pinned.url
        )
    })
}

fn install(bin: &Path, pinned: &Pinned) -> Result<PathBuf> {
    std::fs::create_dir_all(bin)?;
    let archive = bin.join(format!("pdfium-{}.tgz.part", pinned.platform));
    download(pinned.url, &archive, pinned.bytes)?;
    if let Err(error) = verify_archive_file(&archive, pinned.bytes, pinned.sha256) {
        let _ = std::fs::remove_file(&archive);
        return Err(error);
    }
    let library = bin.join(pinned.library);
    let staged = bin.join(format!("{}.part{}", pinned.library, std::process::id()));
    let extracted = extract_member(&archive, pinned.member, &staged);
    let _ = std::fs::remove_file(&archive);
    if let Err(error) = extracted {
        let _ = std::fs::remove_file(&staged);
        return Err(error);
    }
    // The stamp goes last: a crash before this point leaves a library without a stamp, which is
    // downloaded again on the next try
    let _ = std::fs::remove_file(stamp_path(bin));
    if let Err(error) = std::fs::rename(&staged, &library) {
        let _ = std::fs::remove_file(&staged);
        return Err(error).with_context(|| format!("failed to place {library:?}"));
    }
    std::fs::write(stamp_path(bin), stamp_text(pinned))?;
    tracing::info!(path = %library.display(), "PDF renderer installed");
    Ok(library)
}

fn download(url: &str, dest: &Path, expected_bytes: u64) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("Cluaiz-Engine")
        .build()?;
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url} failed"))?
        .error_for_status()?;
    let mut writer = BufWriter::new(File::create(dest)?);
    let copied = std::io::copy(&mut response.take(expected_bytes + 1), &mut writer)?;
    writer.flush()?;
    if copied != expected_bytes {
        bail!("downloaded size mismatch for {url}: expected {expected_bytes} bytes, got {copied}");
    }
    Ok(())
}

/// Copy one regular file out of a gzip tar archive.
fn extract_member(archive: &Path, member: &str, dest: &Path) -> Result<()> {
    let gz = flate2::read::GzDecoder::new(BufReader::new(File::open(archive)?));
    let mut tar = tar::Archive::new(gz);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().trim_start_matches("./").to_owned();
        if path == member && entry.header().entry_type().is_file() {
            let mut out = BufWriter::new(File::create(dest)?);
            std::io::copy(&mut entry, &mut out)?;
            out.flush()?;
            return Ok(());
        }
    }
    bail!("{member} is not inside the PDF renderer archive")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_platform_has_one_pinned_archive_of_the_same_release() {
        for (os, arch, platform) in [
            ("windows", "x86_64", "win-x64"),
            ("linux", "x86_64", "linux-x64"),
            ("linux", "aarch64", "linux-arm64"),
            ("macos", "aarch64", "mac-arm64"),
        ] {
            let pinned = pinned_for(os, arch).expect(platform);
            assert_eq!(pinned.platform, platform);
            assert!(pinned.url.contains(&format!("/download/{RELEASE}/pdfium-{platform}.tgz")));
            assert_eq!(pinned.sha256.len(), 64);
            assert!(pinned.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(pinned.bytes > 3_000_000 && pinned.bytes < 5_000_000);
        }
        assert!(pinned_for("windows", "aarch64").is_none());
        assert!(pinned_for("macos", "x86_64").is_none());
    }

    #[test]
    fn download_is_needed_until_the_library_and_a_matching_stamp_are_both_there() {
        let dir = tempfile::tempdir().unwrap();
        let pinned = &PINNED[0];
        assert_eq!(decide(dir.path(), pinned), Decision::Download);
        std::fs::write(dir.path().join(pinned.library), b"lib").unwrap();
        // library without stamp: an interrupted install
        assert_eq!(decide(dir.path(), pinned), Decision::Download);
        // stamp of an older release
        std::fs::write(stamp_path(dir.path()), "chromium/8152 win-x64 00\n").unwrap();
        assert_eq!(decide(dir.path(), pinned), Decision::Download);
        // stamp of another platform's archive
        std::fs::write(stamp_path(dir.path()), stamp_text(&PINNED[1])).unwrap();
        assert_eq!(decide(dir.path(), pinned), Decision::Download);
        std::fs::write(stamp_path(dir.path()), stamp_text(pinned)).unwrap();
        assert_eq!(decide(dir.path(), pinned), Decision::Ready(dir.path().join(pinned.library)));
        // an empty library file is not ready
        std::fs::write(dir.path().join(pinned.library), b"").unwrap();
        assert_eq!(decide(dir.path(), pinned), Decision::Download);
    }

    #[test]
    fn archive_check_refuses_wrong_size_and_wrong_digest() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.tgz");
        std::fs::write(&file, b"hello").unwrap();
        let sha = format!("{:x}", Sha256::digest(b"hello"));
        verify_archive_file(&file, 5, &sha).unwrap();
        assert!(verify_archive_file(&file, 6, &sha).is_err());
        assert!(verify_archive_file(&file, 5, &"0".repeat(64)).is_err());
    }

    #[test]
    fn only_the_named_member_is_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tgz");
        {
            let gz = flate2::write::GzEncoder::new(File::create(&archive).unwrap(), flate2::Compression::fast());
            let mut builder = tar::Builder::new(gz);
            for (name, body) in [("lib/pdfium.dll.lib", b"stub".as_slice()), ("bin/pdfium.dll", b"real".as_slice())] {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append_data(&mut header, name, body).unwrap();
            }
            builder.into_inner().unwrap().finish().unwrap();
        }
        let out = dir.path().join("pdfium.dll");
        extract_member(&archive, "bin/pdfium.dll", &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"real");
        assert!(extract_member(&archive, "lib/libpdfium.so", &dir.path().join("x")).is_err());
    }

    /// Real download from GitHub into a temporary folder. Run with --ignored.
    #[test]
    #[ignore = "downloads about 4 MB from GitHub"]
    fn live_download_of_this_platforms_pinned_archive() {
        let dir = tempfile::tempdir().unwrap();
        let pinned = pinned_for(std::env::consts::OS, std::env::consts::ARCH).unwrap();
        let path = ensure_in(dir.path(), pinned).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 1_000_000);
        assert_eq!(decide(dir.path(), pinned), Decision::Ready(path));
    }
}
