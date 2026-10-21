//! Opt-in real pooled GGUF vectors; all model/text inputs come from the test environment.
use anyhow::{Context, Result};
use cluaiz_core::{contracts::InvalidEmbeddingInput, media_limits::MediaLimits};
use llama_engine::{ffi::llama_cpp, native::embedding::EmbeddingModel, NativeLlama};

#[test]
#[ignore = "requires CLUAIZ_EMBEDDING_MODELS JSON paths and three CLUAIZ_EMBEDDING_TEST_* texts"]
fn test_real_native_pooled_embeddings_repeat_rank_and_recover() -> Result<()> {
    let paths: Vec<std::path::PathBuf> =
        serde_json::from_str(&std::env::var("CLUAIZ_EMBEDDING_MODELS")?)?;
    assert!(!paths.is_empty());
    let first = std::env::var("CLUAIZ_EMBEDDING_TEST_TEXT")?;
    let other = std::env::var("CLUAIZ_EMBEDDING_TEST_OTHER")?;
    let query = std::env::var("CLUAIZ_EMBEDDING_TEST_QUERY")?;
    for path in paths {
        let header = cluaiz_models::GgufProber::read_header(&path)?;
        println!(
            "REAL GGUF HEADER: {} architecture={:?}",
            path.display(),
            header.metadata.get("general.architecture")
        );
        // SAFETY: native factories take no borrowed handles.
        let (mut model, mut context) = unsafe {
            (
                llama_cpp::llama_model_default_params(),
                llama_cpp::llama_context_default_params(),
            )
        };
        model.n_gpu_layers = 0;
        model.set_mmap(true);
        model.use_extra_bufts = false;
        context.embeddings = 1;
        context.n_ctx = 512;
        context.n_batch = 512;
        context.n_ubatch = 512;
        context.n_threads = 2;
        context.n_threads_batch = 2;
        context.offload_kqv = 0;
        context.op_offload = 0;
        let llama = NativeLlama::load(
            path.to_str().context("UTF-8 model path")?,
            model,
            context,
            &mut Default::default(),
            0,
            0,
            0,
            None,
        )?;
        let mut owner = EmbeddingModel::new(llama)?;
        let limits = MediaLimits::default();
        let (a, count) = owner.embed(&first, limits)?;
        let (repeated, repeated_count) = owner.embed(&first, limits)?;
        assert_eq!(a, repeated);
        assert_eq!(count, repeated_count);
        assert!(count > 0 && !a.is_empty());
        let (b, _) = owner.embed(&other, limits)?;
        assert_ne!(a, b);
        let (q, _) = owner.embed(&query, limits)?;
        let similarity = |v: &[f32]| q.iter().zip(v).map(|(q, v)| q * v).sum::<f32>();
        assert!(
            similarity(&a) > similarity(&b),
            "selected query should rank the related supplied text higher"
        );
        for vector in [&a, &b, &q] {
            assert!(vector.iter().all(|v| v.is_finite()));
            assert!((vector.iter().map(|v| v * v).sum::<f32>().sqrt() - 1.0).abs() < 1e-4);
        }
        assert!(owner
            .embed("", limits)
            .unwrap_err()
            .is::<InvalidEmbeddingInput>());
        // The explicit oversized sequence must fail without truncation or corrupting recovery.
        let oversized = " a".repeat(context.n_ctx as usize * 2);
        let error = owner.embed(&oversized, limits).unwrap_err();
        assert!(error.is::<InvalidEmbeddingInput>(), "{error:#}");
        assert_eq!(owner.embed(&first, limits)?.0, a);
        println!("REAL GGUF EMBEDDING: {} -> {} dimensions, {count} tokens; repeat/rank/overflow/recovery passed", path.display(), a.len());
    }
    Ok(())
}
