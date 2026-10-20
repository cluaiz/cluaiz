//! Manifest-owned multigraph packages; generic ONNX/STT/GGUF grouping remains in hf_hub.
use super::{AssetBundler, HfTreeItem, HfVariant, HuggingFaceHub};
use cluaiz_core::model_assets::{AssetContract, PackageAssets};
use futures_util::StreamExt;
use std::{collections::BTreeSet, path::Path};

pub async fn complete_variants(
    repo: &str,
    items: &[HfTreeItem],
    variants: &mut Vec<HfVariant>,
) -> Result<(), String> {
    reconcile(repo, items, variants, None).await
}

/// The manual file picker uses the same contract resolution as normal installs.
pub async fn complete_custom(
    repo: &str,
    items: &[HfTreeItem],
    custom: HfVariant,
) -> Result<HfVariant, String> {
    let selected = custom.all_files.clone();
    let mut variants = vec![custom];
    reconcile(repo, items, &mut variants, Some(&selected)).await?;
    if variants.len() != 1 {
        return Err(
            "Selected graphs belong to multiple packages; select one runnable package".into(),
        );
    }
    let mut variant = variants.remove(0);
    if let Some(revision) = &variant.revision {
        let inventory = HuggingFaceHub::list_raw_tree_at(repo, revision).await?;
        let mut files: BTreeSet<_> = variant.all_files.into_iter().collect();
        for path in selected {
            if !inventory
                .iter()
                .any(|item| item.path == path && item.r#type.as_deref() == Some("file"))
            {
                return Err(format!(
                    "Selected file is absent in package snapshot: {path}"
                ));
            }
            files.insert(path);
        }
        variant.size_gb = inventory
            .iter()
            .filter(|item| files.contains(&item.path))
            .map(|item| item.size.unwrap_or(0))
            .sum::<u64>() as f64
            / cluaiz_core::GIB_F64;
        variant.all_files = files.into_iter().collect();
    }
    Ok(variant)
}

async fn reconcile(
    repo: &str,
    items: &[HfTreeItem],
    variants: &mut Vec<HfVariant>,
    selection: Option<&[String]>,
) -> Result<(), String> {
    for contract in AssetContract::builtins().map_err(|e| e.to_string())? {
        for item in items.iter().filter(|item| {
            item.r#type.as_deref() == Some("file")
                && Path::new(&item.path).file_name()
                    == Some(std::ffi::OsStr::new(&contract.manifest))
        }) {
            let (value, revision) =
                read_manifest(repo, &item.path, contract.max_manifest_bytes).await?;
            let Some(package) = contract
                .resolve(&value)
                .map_err(|e| format!("{}: {e}", item.path))?
            else {
                continue;
            };
            let root = Path::new(&item.path)
                .parent()
                .unwrap_or_else(|| Path::new(""));
            if selection.is_some_and(|files| {
                !files
                    .iter()
                    .any(|path| path.ends_with(".onnx") && owns_graph(path, root, &package))
            }) {
                continue;
            }
            // The manifest response pins the exact commit. Re-read the tree at that
            // commit so a concurrent upstream update cannot mix manifest and weights.
            let snapshot = HuggingFaceHub::list_raw_tree_at(repo, &revision).await?;
            let bundle = manifest_variant(&item.path, &package, &snapshot, Some(revision))?;
            replace_stages(variants, &item.path, &package, bundle);
        }
    }
    Ok(())
}

fn replace_stages(
    variants: &mut Vec<HfVariant>,
    manifest: &str,
    package: &PackageAssets,
    bundle: HfVariant,
) {
    let root = Path::new(manifest)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    variants.retain(|variant| {
        variant.format_type != "onnx" || !owns_graph(&variant.primary_file, root, package)
    });
    variants.push(bundle);
}

fn owns_graph(path: &str, root: &Path, package: &PackageAssets) -> bool {
    let parent = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
    parent == root
        || package
            .auxiliary_roots
            .iter()
            .any(|folder| parent.starts_with(root.join(folder)))
        || package
            .assets
            .values()
            .any(|asset| Path::new(path) == root.join(asset))
}

/// The graph-header prober remains authoritative for nonstandard external-data names
/// during installation. Here reuse the existing companion suffix matcher for the preview.
pub fn manifest_variant(
    manifest_path: &str,
    package: &PackageAssets,
    items: &[HfTreeItem],
    revision: Option<String>,
) -> Result<HfVariant, String> {
    let root = Path::new(manifest_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let primary = root
        .join(&package.primary)
        .to_string_lossy()
        .replace('\\', "/");
    let mut files = BTreeSet::new();
    for relative in package.required_files() {
        let path = root.join(&relative).to_string_lossy().replace('\\', "/");
        AssetBundler::local_asset_path(&path, &primary)?;
        if !items
            .iter()
            .any(|item| item.path == path && item.r#type.as_deref() == Some("file"))
        {
            return Err(format!(
                "Incomplete package export: required '{path}' is absent in repository"
            ));
        }
        if package.graphs.contains(&relative) {
            let base =
                crate::taxonomy::quantization::UniversalQuantization::strip_onnx_companion_suffix(
                    &path,
                );
            for item in items.iter().filter(|item| crate::taxonomy::quantization::UniversalQuantization::is_onnx_companion_file(&item.path)
                && crate::taxonomy::quantization::UniversalQuantization::strip_onnx_companion_suffix(&item.path) == base) {
                files.insert(item.path.clone());
            }
        }
        files.insert(path);
    }
    let bytes: u64 = items
        .iter()
        .filter(|item| files.contains(&item.path))
        .map(|item| item.size.unwrap_or(0))
        .sum();
    Ok(HfVariant {
        revision,
        variant_id: format!(
            "{} ({})",
            package.manifest,
            package.precision.to_uppercase()
        ),
        format_type: "onnx".into(),
        quant_tag: package.precision.to_uppercase(),
        filename: package.primary.clone(),
        primary_file: primary,
        all_files: files.into_iter().collect(),
        size_gb: bytes as f64 / cluaiz_core::GIB_F64,
    })
}

async fn read_manifest(
    repo: &str,
    path: &str,
    maximum: u64,
) -> Result<(serde_json::Value, String), String> {
    cluaiz_core::model_assets::safe_asset_path(path).map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
    let mut request = client.get(format!("https://huggingface.co/{repo}/resolve/main/{path}"));
    if let Some(token) = HuggingFaceHub::resolve_hf_token() {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Manifest download failed: {}", response.status()));
    }
    let revision = response
        .headers()
        .get("x-repo-commit")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() == 40 && v.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or("Manifest response lacks a valid repository revision")?
        .to_owned();
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if bytes.len() as u64 + chunk.len() as u64 > maximum {
            return Err("Package manifest exceeds metadata limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid package manifest: {e}"))?,
        revision,
    ))
}

#[cfg(test)]
#[path = "package_tests/bundles.rs"]
mod tests;
