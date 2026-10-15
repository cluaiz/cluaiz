use super::*;

#[test]
fn llama_n_ctx_round_trips_through_settings() {
    for (n_ctx, expected) in [
        (0, ContextSetting::Auto),
        (-1, ContextSetting::Max),
        (8192, ContextSetting::Custom(8192)),
    ] {
        let mut opt = OptimizationControl::default();
        opt.set_context_setting(ContextSetting::from_llama_n_ctx(n_ctx));
        assert_eq!(opt.context_setting(), expected);
        assert_eq!(opt.context_setting().to_llama_n_ctx(), n_ctx);
    }
}

#[test]
fn load_fingerprint_ignores_samplers_but_tracks_load_settings() {
    let mut a = ActiveUserSettings::default();
    let base = a.optimization.load_fingerprint();
    a.samplers.temp = Some(0.1);
    assert_eq!(
        a.optimization.load_fingerprint(),
        base,
        "sampler change must not reload the model"
    );
    a.optimization.kv_cache_quantization = KvCacheQuantization::Kv8;
    assert_ne!(
        a.optimization.load_fingerprint(),
        base,
        "KV type change must reload the model"
    );
    let mut b = ActiveUserSettings::default();
    b.optimization
        .set_context_setting(ContextSetting::Custom(8192));
    assert_ne!(
        b.optimization.load_fingerprint(),
        base,
        "context change must reload the model"
    );
}

#[test]
fn thinking_budget_names_numbers_and_auto() {
    let parse = |v: serde_json::Value| serde_json::from_value::<ThinkingBudget>(v);
    assert_eq!(
        parse(serde_json::json!("low")).unwrap(),
        ThinkingBudget::Low
    );
    assert_eq!(
        parse(serde_json::json!("High")).unwrap(),
        ThinkingBudget::High
    );
    assert_eq!(
        parse(serde_json::json!(2500)).unwrap(),
        ThinkingBudget::Tokens(2500)
    );
    assert_eq!(
        parse(serde_json::json!("2500")).unwrap(),
        ThinkingBudget::Tokens(2500)
    );
    assert_eq!(
        parse(serde_json::json!(null)).unwrap(),
        ThinkingBudget::Auto
    );
    assert!(parse(serde_json::json!("sometimes")).is_err());
    assert!(parse(serde_json::json!(-5)).is_err());
    // round trip keeps the user's choice (names stay names, numbers stay numbers)
    assert_eq!(
        serde_json::to_value(ThinkingBudget::Medium).unwrap(),
        serde_json::json!("medium")
    );
    assert_eq!(
        serde_json::to_value(ThinkingBudget::Tokens(800)).unwrap(),
        serde_json::json!(800)
    );
    // room = n_ctx - prompt (1000) - answer reserve (n_ctx / 4, 256..2048)
    let p = ThinkingPresets::default();
    let room = |n_ctx: usize| n_ctx - 1000 - (n_ctx / 4).clamp(256, 2048);
    let t = |b: ThinkingBudget, n_ctx: usize| b.limit(room(n_ctx), &p).tokens;
    use ThinkingBudget::*;
    for (n_ctx, low, medium, high, max) in [
        (4096, 518, 1036, 1554, 2072),
        (32768, 2048, 8192, 22290, 29720),
        (65536, 2048, 8192, 32768, 62488),
        (131072, 2048, 8192, 32768, 128024),
        (262144, 2048, 8192, 32768, 259096),
        (1048576, 2048, 8192, 32768, 1045528),
    ] {
        assert_eq!(
            (
                t(Low, n_ctx),
                t(Medium, n_ctx),
                t(High, n_ctx),
                t(Max, n_ctx)
            ),
            (low, medium, high, max),
            "n_ctx {}",
            n_ctx
        );
        assert_eq!(t(Auto, n_ctx), high, "auto = high at n_ctx {}", n_ctx);
    }
    // a user number bigger than the room is lowered so the answer still fits
    assert_eq!(
        Tokens(100_000).limit(2072, &p),
        ThinkingLimit {
            tokens: 2072,
            capped: true
        }
    );
    assert_eq!(
        Tokens(500).limit(2072, &p),
        ThinkingLimit {
            tokens: 500,
            capped: false
        }
    );
    // no room: no thinking, straight to the answer
    assert_eq!(High.limit(0, &p).tokens, 0);
    // default effort is saved with the settings and is not a reload reason
    let mut opt = OptimizationControl::default();
    let fp = opt.load_fingerprint();
    opt = opt
        .merged_with(&serde_json::json!({ "reasoning_effort": 3000 }))
        .unwrap();
    assert_eq!(opt.reasoning_effort, ThinkingBudget::Tokens(3000));
    assert_eq!(opt.load_fingerprint(), fp);
}

#[test]
fn onnx_settings_partial_update() {
    let saved = OnnxSettings::default();
    let updated = saved
        .merged_with(&serde_json::json!({ "n_gpu_layers": 0 }))
        .unwrap();
    assert_eq!(updated.n_gpu_layers, 0);
    assert_eq!(updated.arena_extend_strategy, saved.arena_extend_strategy);
    assert!(saved
        .merged_with(&serde_json::json!({ "graph_optimization_level": "ORT_ENABLE_ALL" }))
        .is_err());
    assert!(saved
        .merged_with(&serde_json::json!({ "n_gpu_layers": "many" }))
        .is_err());
}

#[test]
fn field_list_matches_the_struct() {
    let serialized = serde_json::to_value(OptimizationControl::default()).unwrap();
    let mut fields: Vec<&str> = serialized
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    let mut listed = OPTIMIZATION_FIELDS.to_vec();
    fields.sort_unstable();
    listed.sort_unstable();
    assert_eq!(
        fields, listed,
        "OPTIMIZATION_FIELDS must list every OptimizationControl field"
    );
    // an old settings.json that still has removed keys loads (they are dropped on the next save)
    let old: OptimizationControl = serde_json::from_value(
        serde_json::json!({ "speculative_decoding": "On", "think_mode": "On" }),
    )
    .unwrap();
    assert_eq!(old.think_mode, FeatureState::On);
}

#[test]
fn partial_update_keeps_other_settings() {
    let saved = OptimizationControl {
        kv_cache_quantization: KvCacheQuantization::Kv8,
        flash_attention: FeatureState::On,
        custom_vram_buffer_gb: Some(0.5),
        ..Default::default()
    };

    // what the app sends: one key, lowercase value
    let updated = saved
        .merged_with(&serde_json::json!({ "think_mode": "on" }))
        .unwrap();
    assert_eq!(updated.think_mode, FeatureState::On);
    assert_eq!(
        updated.kv_cache_quantization,
        KvCacheQuantization::Kv8,
        "other settings must stay"
    );
    assert_eq!(updated.flash_attention, FeatureState::On);
    assert_eq!(updated.custom_vram_buffer_gb, Some(0.5));

    // a key given by its alias replaces the saved field
    let updated = saved
        .merged_with(&serde_json::json!({ "n_ctx": 8192 }))
        .unwrap();
    assert_eq!(updated.context_setting(), ContextSetting::Custom(8192));

    // bad values and unknown or removed settings are an error, not silently ignored
    assert!(saved
        .merged_with(&serde_json::json!({ "think_mode": "sometimes" }))
        .is_err());
    assert!(saved
        .merged_with(&serde_json::json!({ "speculative_decoding": "On" }))
        .is_err());
    assert!(saved
        .merged_with(&serde_json::json!({ "flash_atention": "On" }))
        .is_err());
    assert!(saved
        .merged_with(&serde_json::json!(["think_mode"]))
        .is_err());
}

#[test]
fn request_llama_params_override_settings() {
    let p: LlamaLoadParams = serde_json::from_str(
        r#"{"n_ctx": 8192, "ngl": 20, "cache_type_k": "q8_0", "cache_type_v": "q8_0", "flash_attn": "on", "n_threads": 6}"#,
    ).unwrap();
    let mut opt = OptimizationControl::default();
    p.apply_to(&mut opt).unwrap();
    assert_eq!(opt.context_setting(), ContextSetting::Custom(8192));
    assert_eq!(opt.n_gpu_layers, Some(20));
    assert_eq!(opt.kv_cache_quantization, KvCacheQuantization::Kv8);
    assert_eq!(opt.flash_attention, FeatureState::On);
    assert_eq!(opt.resolved_threads(8), (6, 6));
    assert!(opt.is_manual_placement());
    assert_ne!(
        opt.load_fingerprint(),
        OptimizationControl::default().load_fingerprint()
    );

    // Missing fields keep auto; typos are rejected instead of silently ignored
    let mut auto = OptimizationControl::default();
    LlamaLoadParams::default().apply_to(&mut auto).unwrap();
    assert_eq!(auto, OptimizationControl::default());
    assert!(serde_json::from_str::<LlamaLoadParams>(r#"{"n_gpu_layer": 5}"#).is_err());
    let bad = LlamaLoadParams {
        cache_type_k: Some("q8_0".into()),
        cache_type_v: Some("f16".into()),
        ..Default::default()
    };
    assert!(bad.apply_to(&mut OptimizationControl::default()).is_err());
}
