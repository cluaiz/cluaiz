//! Download metadata only: no model installation, vault writes or inference.
use cluaiz_models::fetcher::{hf_hub::HfRepositoryMetadata, AssetBundler};
use cluaiz_models::{HfTreeItem, HfVariant, HuggingFaceHub};

#[test]
fn architecture_uses_declared_gguf_then_config_without_generic_guess() {
    for (value, expected) in [
        (
            serde_json::json!({"gguf":{"architecture":"dense_family"},"config":{"model_type":"other"}}),
            Some("dense_family"),
        ),
        (
            serde_json::json!({"config":{"model_type":"nested_family"}}),
            Some("nested_family"),
        ),
        (
            serde_json::json!({"config":{"text_config":{"model_type":"text_family"}}}),
            Some("text_family"),
        ),
        (
            serde_json::json!({"config":{"model_type":"","architectures":["DeclaredDecoder"]}}),
            Some("DeclaredDecoder"),
        ),
        (serde_json::json!({"config":{"model_type":42}}), None),
        (serde_json::json!({}), None),
    ] {
        let metadata: HfRepositoryMetadata = serde_json::from_value(value).unwrap();
        assert_eq!(metadata.architecture(), expected);
    }
}

#[test]
fn bundle_total_includes_shards_companions_metadata_once() {
    let items = [
        "main-00001-of-00002.gguf",
        "main-00002-of-00002.gguf",
        "mmproj.gguf",
        "mtp.gguf",
        "config.json",
        "unselected.gguf",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, path)| HfTreeItem {
        path: path.into(),
        size: Some((index as u64 + 1) * 100),
        r#type: Some("file".into()),
    })
    .collect::<Vec<_>>();
    let mut files = items[..5]
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    files.push(items[0].path.clone());
    assert_eq!(
        AssetBundler::bundle_size_gb(&items, &files),
        1500.0 / cluaiz_core::GIB_F64
    );
    let variant = HfVariant {
        revision: None,
        variant_id: "fixture".into(),
        format_type: "gguf".into(),
        quant_tag: "Q4_0".into(),
        primary_file: files[0].clone(),
        filename: files[0].clone(),
        all_files: files.clone(),
        size_gb: AssetBundler::bundle_size_gb(&items, &files),
    };
    let manifest = HuggingFaceHub::build_manifest("owner/model", &variant, None, &[]);
    assert_eq!(manifest.download_size_gb, variant.size_gb);
    assert_eq!(manifest.architecture, "Unknown");
}

#[tokio::test]
#[ignore = "read-only HF API/tree check; requires CLUAIZ_PREVIEW_REPO"]
async fn live_download_preview_metadata() {
    let repo = std::env::var("CLUAIZ_PREVIEW_REPO").expect("Set a public HF repository");
    let metadata = HuggingFaceHub::declared_metadata(&repo).await;
    let variants = HuggingFaceHub::list_variants(&repo).await.unwrap();
    let inventory = HuggingFaceHub::list_raw_tree(&repo).await.unwrap();
    let architecture = metadata
        .architecture()
        .expect("Upstream architecture missing");
    println!("architecture={architecture}; variants={}", variants.len());
    for variant in &variants {
        let expected = AssetBundler::bundle_size_gb(&inventory, &variant.all_files);
        assert_eq!(variant.size_gb, expected);
        assert!(
            variant.all_files.len() > 1,
            "Use a companion-bearing repository"
        );
        println!(
            "{}: {:.4} GiB; {} files",
            variant.primary_file,
            variant.size_gb,
            variant.all_files.len()
        );
    }
}
