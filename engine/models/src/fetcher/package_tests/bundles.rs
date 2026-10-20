use super::*;

fn fixture(root: &str) -> (PackageAssets, Vec<HfTreeItem>) {
    let value = crate::test_fixtures::asset_manifest();
    let package = AssetContract::builtins()
        .unwrap()
        .remove(0)
        .resolve(&value)
        .unwrap()
        .unwrap();
    let mut paths = package.required_files();
    for graph in [
        "slow_ar_int4.onnx",
        "fast_ar_int4.onnx",
        "codec_decoder_fp16.onnx",
        "registration/codec_encoder_fp16.onnx",
        "fast_ar_fp16.onnx",
    ] {
        paths.push(graph.into());
        paths.push(format!("{graph}.data"));
    }
    paths.push("other/encoder_model.onnx".into());
    paths.sort();
    paths.dedup();
    let items = paths
        .into_iter()
        .map(|path| HfTreeItem {
            path: format!("{root}{path}"),
            size: Some(10),
            r#type: Some("file".into()),
        })
        .collect();
    (package, items)
}

#[test]
fn test_complete_bundle_replaces_stages_keeps_unrelated_graphs_and_excludes_optional_encoder() {
    for root in ["", "exports/mobile/"] {
        let (package, items) = fixture(root);
        let mut variants = HuggingFaceHub::bundle_onnx_variants(&items, &[]);
        variants.push(HfVariant {
            revision: None,
            variant_id: "untouched.gguf".into(),
            format_type: "gguf".into(),
            quant_tag: "Q4_K_M".into(),
            primary_file: "untouched.gguf".into(),
            filename: "untouched.gguf".into(),
            all_files: vec!["untouched.gguf".into(), "mmproj.gguf".into()],
            size_gb: 1.0,
        });
        let manifest = format!("{root}runtime_manifest.json");
        let bundle = manifest_variant(&manifest, &package, &items, Some("a".repeat(40))).unwrap();
        assert_eq!(bundle.all_files.len(), 8);
        assert_eq!(bundle.size_gb, 80.0 / cluaiz_core::GIB_F64);
        assert!(bundle
            .all_files
            .contains(&format!("{root}codec_decoder_fp16.onnx.data")));
        assert!(!bundle
            .all_files
            .iter()
            .any(|p| p.contains("codec_encoder") || p.contains("fast_ar_fp16")));
        replace_stages(&mut variants, &manifest, &package, bundle);
        assert_eq!(variants.len(), 3, "stages exposed as models: {variants:?}");
        assert_eq!(
            variants
                .iter()
                .find(|v| v.format_type == "gguf")
                .unwrap()
                .all_files,
            ["untouched.gguf", "mmproj.gguf"]
        );
        assert!(variants
            .iter()
            .any(|v| v.primary_file == format!("{root}other/encoder_model.onnx")));
    }
}

#[test]
fn test_repository_missing_stage_fails_before_install() {
    let (package, mut items) = fixture("");
    items.retain(|item| item.path != "slow_ar_int4.onnx");
    let error = manifest_variant("runtime_manifest.json", &package, &items, None).unwrap_err();
    assert!(error.contains("slow_ar_int4.onnx"));
}

#[test]
fn test_revision_and_graph_parent_are_preserved_without_registry_schema_change() {
    let (package, items) = fixture("onnx/");
    let revision = "a".repeat(40);
    let bundle = manifest_variant(
        "onnx/runtime_manifest.json",
        &package,
        &items,
        Some(revision.clone()),
    )
    .unwrap();
    let manifest = HuggingFaceHub::build_manifest("arbitrary-owner/arbitrary-name", &bundle, None);
    assert_eq!(HuggingFaceHub::manifest_revision(&manifest), revision);
    assert_eq!(
        HuggingFaceHub::primary_repository_path(&manifest),
        "onnx/slow_ar_int4.onnx"
    );
    assert_eq!(HuggingFaceHub::asset_url(&manifest, "onnx/fast_ar_int4.onnx"),
        format!("https://huggingface.co/arbitrary-owner/arbitrary-name/resolve/{revision}/onnx/fast_ar_int4.onnx"));
}
