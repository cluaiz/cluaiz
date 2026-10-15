use cluaiz_media::{
    documents::{self, chunker::SemanticChunker, DEFAULT_CHUNK_BYTES},
    source::SourceResolver,
};
use cluaiz_runtime::{
    onnx_host::{EmbeddingInput, OnnxModel},
    EngineHost,
};
use color_eyre::eyre::{eyre, Result};
use colored::Colorize;

/// User-facing ingestion reuses the document service and the configured ONNX owner.
pub async fn execute(file_path: &str) -> Result<()> {
    let settings = cluaiz_core::Settings::current();
    let resolver =
        SourceResolver::from_environment(settings.media).map_err(|error| eyre!("{error:#}"))?;
    // A direct CLI invocation is the user's explicit local-file read.
    let source = resolver
        .resolve(file_path, |_| Ok(()))
        .await
        .map_err(|error| eyre!("{error:#}"))?;
    let bytes = source
        .read_bytes(settings.media.limits)
        .await
        .map_err(|error| eyre!("{error:#}"))?;
    let format = source.format.clone();
    drop(source);
    let limits = settings.media.limits;
    let is_image = format.mime_type.starts_with("image/");
    let inputs = if is_image {
        let image = cluaiz_media::image::decode_image(&bytes, limits)
            .map_err(|error| eyre!("{error:#}"))?;
        vec![EmbeddingInput::Image {
            image,
            text: String::new(),
        }]
    } else {
        let document = tokio::task::spawn_blocking(move || {
            let result = documents::extract(&bytes, &format, limits);
            documents::release_native_providers()?;
            result
        })
        .await?
        .map_err(|error| eyre!("{error:#}"))?;
        for part in &document.parts {
            if !part.omissions.is_empty() {
                eprintln!(
                    "Document page {:?}: {}",
                    part.page,
                    part.omissions.join(", ")
                );
            }
        }
        let text = document
            .text(limits, true)
            .map_err(|error| eyre!("{error:#}"))?;
        let chunks = SemanticChunker::chunk(&text, DEFAULT_CHUNK_BYTES)
            .map_err(|error| eyre!("{error:#}"))?;
        println!("{}: {} extracted chunks", file_path.bold(), chunks.len());
        for (index, chunk) in chunks.iter().enumerate().take(5) {
            println!("Chunk {}:\n{}", index + 1, chunk);
        }
        chunks.into_iter().map(EmbeddingInput::Text).collect()
    };
    let schema = cluaiz_security::PermissionSchema::load();
    let embedding_id = schema
        .active_slots
        .get("embedding_slot")
        .and_then(|slot| slot.model_id.as_deref());
    let selected = if is_image {
        schema
            .active_slots
            .get("ingest_slot")
            .and_then(|slot| slot.model_id.as_deref())
            .or(embedding_id)
    } else {
        embedding_id
    };
    let Some(id) = selected else {
        if is_image {
            return Err(eyre!(
                "Configure an image embedding model in ingest_slot or embedding_slot"
            ));
        }
        println!("Text extracted. Configure an embedding model to also generate vectors.");
        return Ok(());
    };
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    let entry = registry
        .installed_models
        .get(id)
        .ok_or_else(|| eyre!("Configured embedding model is not installed: {id}"))?;
    let model = OnnxModel::from_package(entry).map_err(|error| eyre!("{error:#}"))?;
    let host = EngineHost::new();
    let results = host
        .onnx
        .embed(model, inputs, settings, Some(0))
        .await
        .map_err(|error| eyre!("{error:#}"))?;
    println!(
        "Generated {} real embedding vectors using {}",
        results.len(),
        id
    );
    Ok(())
}
