//! Package existing native media assets, reusing the runtime manifest verifier.
use anyhow::{bail, Context, Result};
use cluaiz_media::native_assets::{NativeAssets, MANIFEST_NAME};
use std::path::{Path, PathBuf};

pub fn run(args: &[String]) -> Result<()> {
    let mut source = None;
    let mut output = None;
    for pair in args.chunks(2) {
        if pair.len() != 2 {
            bail!("Use media-assets --source <package> --output <new-directory>");
        }
        match pair[0].as_str() {
            "--source" if source.is_none() => source = Some(PathBuf::from(&pair[1])),
            "--output" if output.is_none() => output = Some(PathBuf::from(&pair[1])),
            _ => bail!(
                "Unknown or repeated native media packaging argument: {}",
                pair[0]
            ),
        }
    }
    let count = stage(
        &source.context("Native media source is required")?,
        &output.context("Native media output is required")?,
    )?;
    println!("Native media package staged: {count} verified files");
    Ok(())
}

/// Output must be new; existing installations/release packages are never replaced.
fn stage(source: &Path, output: &Path) -> Result<usize> {
    let assets = NativeAssets::read(source)?;
    assets.verify()?;
    if output.exists() {
        bail!("Native media output already exists; choose a new staging directory");
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let name = output
        .file_name()
        .context("Native media output requires a directory name")?;
    let output = parent.join(name);
    let staging = tempfile::tempdir_in(&parent)?;
    for asset in assets.manifest.files.values() {
        let source = assets.path(&asset.path)?;
        let target = staging.path().join(&asset.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, target)?;
    }
    std::fs::copy(
        assets.root.join(MANIFEST_NAME),
        staging.path().join(MANIFEST_NAME),
    )?;
    NativeAssets::read(staging.path())?.verify()?;
    // All validation precedes the final directory rename. Errors clean temporary files.
    std::fs::rename(staging.path(), &output)?;
    Ok(assets.manifest.files.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_media_packaging_copies_verified_assets_and_refuses_damage_or_overwrite() -> Result<()> {
        let source = tempfile::tempdir()?;
        let target = tempfile::tempdir()?;
        let bytes = b"bounded real package file";
        let manifest = serde_json::json!({"schema_version":1,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,
            "files":{"notice":{"path":"licenses/NOTICE","sha256":"","source":"test"}},"ocr":null,"pdf":null});
        // Digest is generated from the fixture, never trusted as a fixed test result.
        use sha2::{Digest, Sha256};
        let mut manifest = manifest;
        manifest["files"]["notice"]["sha256"] = format!("{:x}", Sha256::digest(bytes)).into();
        std::fs::create_dir_all(source.path().join("licenses"))?;
        std::fs::write(source.path().join("licenses/NOTICE"), bytes)?;
        std::fs::write(source.path().join(MANIFEST_NAME), manifest.to_string())?;
        let output = target.path().join("media");
        assert_eq!(stage(source.path(), &output)?, 1);
        assert_eq!(std::fs::read(output.join("licenses/NOTICE"))?, bytes);
        assert!(stage(source.path(), &output).is_err());
        std::fs::write(source.path().join("licenses/NOTICE"), b"damaged")?;
        assert!(stage(source.path(), &target.path().join("damaged")).is_err());
        assert!(!target.path().join("damaged").exists());
        Ok(())
    }
}
