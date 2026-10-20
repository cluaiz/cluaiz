//! Opt-in fresh upstream installation into an explicitly isolated CLUAIZ_HOME.
use anyhow::{Context, Result};
use cluaiz_models::{HuggingFaceHub, ModelDownloader};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

#[tokio::test]
#[ignore = "requires CLUAIZ_PACKAGE_REPO and CLUAIZ_PACKAGE_SELECTED_GRAPH; network metadata only"]
async fn actual_component_selection_and_legacy_primary_resolve_one_package() -> Result<()> {
    let repo = std::env::var("CLUAIZ_PACKAGE_REPO")?;
    let graph = std::env::var("CLUAIZ_PACKAGE_SELECTED_GRAPH")?;
    let tree = HuggingFaceHub::list_raw_tree(&repo)
        .await
        .map_err(anyhow::Error::msg)?;
    let custom = cluaiz_models::HfVariant {
        revision: None,
        variant_id: "custom".into(),
        format_type: "onnx".into(),
        quant_tag: "CUSTOM".into(),
        primary_file: graph.clone(),
        all_files: vec![graph.clone()],
        filename: graph.clone(),
        size_gb: 0.0,
    };
    let old = HuggingFaceHub::build_manifest(&repo, &custom, None);
    let manual = cluaiz_models::fetcher::package_bundler::complete_custom(&repo, &tree, custom)
        .await
        .map_err(anyhow::Error::msg)?;
    let (repair, files) = HuggingFaceHub::bundle_for_manifest(&old)
        .await
        .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(manual.revision.is_some());
    anyhow::ensure!(manual.all_files == files);
    anyhow::ensure!(files.len() > 1 && files.contains(&graph));
    anyhow::ensure!(
        repair.huggingface_filename != graph,
        "Component remained the model entrypoint"
    );
    println!(
        "ACTUAL MANUAL COMPONENT + LEGACY PRIMARY REPAIR PLAN PASS: primary={} files={files:?}",
        repair.huggingface_filename
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated CLUAIZ_HOME, CLUAIZ_PACKAGE_REPO and explicit CLUAIZ_ALLOW_PACKAGE_DOWNLOAD=1"]
async fn actual_fresh_package_install_and_repair_preserve_valid_files() -> Result<()> {
    anyhow::ensure!(
        std::env::var("CLUAIZ_ALLOW_PACKAGE_DOWNLOAD").as_deref() == Ok("1"),
        "Download not enabled"
    );
    let home = PathBuf::from(std::env::var("CLUAIZ_HOME")?);
    anyhow::ensure!(
        home.is_absolute() && !home.exists(),
        "Fixture requires a fresh, isolated home"
    );
    let repo = std::env::var("CLUAIZ_PACKAGE_REPO")?;
    let variants = HuggingFaceHub::list_variants(&repo)
        .await
        .map_err(anyhow::Error::msg)?;
    let mut packages = variants
        .iter()
        .filter(|v| v.revision.is_some() && v.format_type == "onnx");
    let variant = packages.next().context("Missing manifest-owned package")?;
    anyhow::ensure!(
        packages.next().is_none(),
        "Fixture requires exactly one export"
    );
    println!(
        "FRESH PLAN {} revision={:?} files={:?} bytes_estimate={}",
        variant.variant_id,
        variant.revision,
        variant.all_files,
        variant.size_gb * cluaiz_core::GIB_F64
    );
    let mut manifest = HuggingFaceHub::build_manifest(&repo, variant, None);
    manifest.huggingface_filename = variant
        .primary_file
        .rsplit('/')
        .next()
        .context("Primary")?
        .into();
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let abort = Arc::new(AtomicBool::new(false));
    let primary =
        ModelDownloader::install(&manifest, &variant.all_files, tx.clone(), abort.clone())
            .await
            .map_err(anyhow::Error::msg)?;
    ModelDownloader::validate_package(&primary).map_err(anyhow::Error::msg)?;
    let dir = primary.parent().context("Package directory")?;
    let package = cluaiz_models::capabilities::PackageCapabilities::inspect(&primary)?;
    anyhow::ensure!(package.asset_contract.is_some() && package.missing_onnx_assets().is_empty());
    // Compare actual downloaded LFS bytes with the same pinned upstream tree.
    let inventory: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "https://huggingface.co/api/models/{repo}/tree/{}?recursive=true",
            variant.revision.as_deref().context("Revision")?
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut hashes = Vec::new();
    for remote in inventory.as_array().context("Tree")? {
        let Some(path) = remote["path"].as_str() else {
            continue;
        };
        if !variant.all_files.iter().any(|selected| selected == path) {
            continue;
        }
        let Some(expected) = remote["lfs"]["oid"].as_str() else {
            continue;
        };
        let local =
            cluaiz_models::fetcher::AssetBundler::local_asset_path(path, &variant.primary_file)
                .map_err(anyhow::Error::msg)?;
        let mut file = std::fs::File::open(dir.join(&local))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        let actual = format!("{:x}", hasher.finalize());
        anyhow::ensure!(actual == expected, "LFS hash differs for {path}");
        println!("LFS SHA256 MATCH {path} {actual}");
        hashes.push(local);
    }
    anyhow::ensure!(!hashes.is_empty(), "No actual upstream LFS hashes checked");
    let retained = dir.join(&hashes[0]);
    let stamp = retained.metadata()?.modified()?;
    let contract = package.asset_contract.context("Contract")?;
    let tokenizer = dir.join(contract.file("tokenizer")?);
    std::fs::remove_file(&tokenizer)?; // fixture-owned package only
    anyhow::ensure!(ModelDownloader::validate_package(&primary).is_err());
    ModelDownloader::install(&manifest, &variant.all_files, tx.clone(), abort)
        .await
        .map_err(anyhow::Error::msg)?;
    ModelDownloader::validate_package(&primary).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        retained.metadata()?.modified()? == stamp,
        "Valid weights were redownloaded"
    );
    println!(
        "FRESH INSTALL + MISSING TOKENIZER REPAIR PASS: {}",
        dir.display()
    );
    drop(tx);
    drain.await?;
    Ok(())
}
