use super::*;

fn manifest() -> Value {
    serde_json::json!({
        "model_family":"audio8_tts", "activation_dtype":"float16",
        "slow_logits_layout":"semantic_then_eos", "kv_attention_layout":"valid_prefix",
        "default_precision":"int4", "available_precisions":["int4"],
        "default_codec_precision":"fp16", "codec_models":{"fp16":"codec_decoder_fp16.onnx"}
    })
}

#[test]
fn test_manifest_resolves_mixed_precision_required_assets_only() -> Result<()> {
    let contract = AssetContract::builtins()?.remove(0);
    let package = contract.resolve(&manifest())?.context("not recognized")?;
    assert_eq!(package.primary, "slow_ar_int4.onnx");
    assert_eq!(package.file("decoder")?, "codec_decoder_fp16.onnx");
    assert_eq!(package.required_files().len(), 5);
    assert!(!package
        .required_files()
        .iter()
        .any(|p| p.contains("encoder")));
    Ok(())
}

#[test]
fn test_manifest_rejects_unsupported_precision_layout_and_missing_codec() -> Result<()> {
    let contract = AssetContract::builtins()?.remove(0);
    for pointer in [
        "activation_dtype",
        "default_precision",
        "default_codec_precision",
    ] {
        let mut value = manifest();
        value[pointer] = Value::String("unknown".into());
        assert!(contract.resolve(&value).is_err(), "accepted {pointer}");
    }
    assert!(contract
        .resolve(&serde_json::json!({"model_family":"another_export"}))?
        .is_none());
    Ok(())
}

#[test]
fn test_manifest_rejects_asset_escape_paths() -> Result<()> {
    let contract = AssetContract::builtins()?.remove(0);
    for path in [
        "../outside.onnx",
        "/absolute.onnx",
        "C:/outside.onnx",
        "a\\b.onnx",
        "",
    ] {
        let mut value = manifest();
        value["codec_models"]["fp16"] = path.into();
        assert!(contract.resolve(&value).is_err(), "accepted {path}");
    }
    assert!(safe_asset_path("weights/shard.data").is_ok());
    Ok(())
}
