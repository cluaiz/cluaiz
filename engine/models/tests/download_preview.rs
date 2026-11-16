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

#[test]
fn a_split_set_is_one_variant_with_every_shard() {
    // unsloth/Qwen3.8-Flash-Next-GGUF: one folder per quant, 8 BF16 and 6 Q8_0 shards, MTP heads beside them
    let mut paths = vec!["mmproj-F16.gguf".to_string(), "MTP/mtp-Qwen3.8-Flash-Next-BF16.gguf".into(), "MTP/mtp-Qwen3.8-Flash-Next-Q8_0.gguf".into()];
    paths.extend((1..=8).map(|i| format!("BF16/Qwen3.8-Flash-Next-BF16-{i:05}-of-00008.gguf")));
    paths.extend((1..=6).map(|i| format!("Q8_0/Qwen3.8-Flash-Next-Q8_0-{i:05}-of-00006.gguf")));
    let items: Vec<HfTreeItem> = paths
        .iter()
        .map(|path| HfTreeItem { path: path.clone(), size: Some(1 << 30), r#type: Some("file".into()) })
        .collect();
    let variants = HuggingFaceHub::variants_from_tree(&items);
    assert_eq!(variants.len(), 2, "{:?}", variants.iter().map(|v| &v.variant_id).collect::<Vec<_>>());
    let bf16 = variants.iter().find(|v| v.quant_tag == "BF16").unwrap();
    assert_eq!(bf16.primary_file, "BF16/Qwen3.8-Flash-Next-BF16-00001-of-00008.gguf");
    assert_eq!(bf16.all_files.iter().filter(|f| f.starts_with("BF16/")).count(), 8);
    assert!(bf16.all_files.contains(&"MTP/mtp-Qwen3.8-Flash-Next-BF16.gguf".to_string()));
    // 8 shards + mmproj + MTP, 1 GiB each
    assert!((bf16.size_gb - 10.0).abs() < 0.01, "{}", bf16.size_gb);
    let q8 = variants.iter().find(|v| v.quant_tag == "Q8_0").unwrap();
    assert_eq!(q8.all_files.iter().filter(|f| f.starts_with("Q8_0/")).count(), 6);
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
