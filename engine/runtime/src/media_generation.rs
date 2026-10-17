//! Serialized native media execution; package checks and request-derived admission live here.

use super::{EngineHost, GenerationRequest, HostResponse, MediaGeneration};
use crate::media_admission::MediaLease;
use cluaiz_core::{
    chat::ChatEnvelope, contracts::StreamCallback, media::PreparedPart, ActiveUserSettings,
};
use cluaiz_models::StructuralDNA;
use llama_engine::native::session::NativeSession;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};

impl EngineHost {
    /// Typed media stays owned until the worker ends; queued inputs retain model identity.
    pub async fn dispatch_prepared(
        &self,
        request: ChatEnvelope<Vec<PreparedPart>>,
        max_tokens: usize,
        model_path: &Path,
        settings: &ActiveUserSettings,
        cancel: Option<Arc<AtomicBool>>,
        skip: Option<Arc<AtomicBool>>,
    ) -> HostResponse {
        self.dispatch_prepared_with_progress(
            request, max_tokens, model_path, settings, cancel, skip, None, None, None,
        )
        .await
    }

    /// API preparation and native execution share the same optional admission owner.
    #[allow(clippy::too_many_arguments)]
    pub async fn dispatch_prepared_with_progress(
        &self,
        request: ChatEnvelope<Vec<PreparedPart>>,
        max_tokens: usize,
        model_path: &Path,
        settings: &ActiveUserSettings,
        cancel: Option<Arc<AtomicBool>>,
        skip: Option<Arc<AtomicBool>>,
        progress: Option<cluaiz_core::processing::ProcessingReporter>,
        media_lease: Option<MediaLease>,
        chat: Option<&str>,
    ) -> HostResponse {
        if request
            .messages
            .iter()
            .flat_map(|m| &m.content)
            .all(|part| matches!(part, PreparedPart::Text { .. }))
        {
            // Extracted documents are text inputs and require no vision projector.
            let request = request
                .try_map_messages(|messages| async move {
                    messages
                        .into_iter()
                        .map(|message| {
                            let mut text = String::new();
                            for part in message.content {
                                match part {
                                    PreparedPart::Text { text: value } => text.push_str(&value),
                                    _ => anyhow::bail!("Non-text part in text preparation"),
                                }
                            }
                            Ok(cluaiz_core::ChatMessage {
                                role: message.role,
                                content: text,
                            })
                        })
                        .collect::<anyhow::Result<Vec<_>>>()
                })
                .await;
            let request = match request {
                Ok(request) => request,
                Err(error) => return HostResponse::Error(error.to_string()),
            };
            return self
                .dispatch_request(
                    GenerationRequest::Text {
                        prompt: request.to_prompt(),
                        expected_key: Some(Self::load_key(&model_path.to_string_lossy(), settings)),
                        media_lease,
                        chat: chat.map(str::to_owned),
                    },
                    max_tokens,
                    cancel,
                    skip,
                )
                .await;
        }
        self.dispatch_request(
            GenerationRequest::Media(Box::new(MediaGeneration {
                request,
                expected_key: Self::load_key(&model_path.to_string_lossy(), settings),
                settings: settings.clone(),
                progress,
                media_lease,
                chat: chat.map(str::to_owned),
            })),
            max_tokens,
            cancel,
            skip,
        )
        .await
    }
}

pub(crate) fn execute(
    session: &mut NativeSession,
    model: &Path,
    request: ChatEnvelope<Vec<PreparedPart>>,
    settings: &ActiveUserSettings,
    max_tokens: usize,
    dna: &StructuralDNA,
    callback: StreamCallback,
) -> anyhow::Result<Vec<i32>> {
    let package = cluaiz_models::capabilities::PackageCapabilities::inspect(model)?;
    let supported = package.supported_inputs();
    if request
        .messages
        .iter()
        .flat_map(|m| &m.content)
        .any(|part| !supported.contains(&part.modality()))
    {
        anyhow::bail!("Selected model does not declare support for every request input");
    }
    let projector = package
        .projector
        .ok_or_else(|| anyhow::anyhow!("Compatible media projector is missing or ambiguous"))?;
    let needs = cluaiz_plan::media_budget::MediaRequirements {
        weights_bytes: std::fs::metadata(&projector.path)?.len(),
        workspace_bytes: settings.media.projector_workspace_bytes.get(),
        decoded_bytes: PreparedPart::request_bytes(&request.messages)?,
    };
    session.ensure_projector(&projector.path, needs, settings, session.gpu_layers() != 0)?;
    session.stream_prepared(request, max_tokens, dna, settings, callback)?;
    Ok(Vec::new()) // Media embeddings never become text-only prefix history.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_native_worker_keeps_document_and_image_admission_after_caller_drop() {
        let host = EngineHost::new();
        let image = cluaiz_core::media::DecodedImage::new(1, 1, Arc::from([255, 0, 0])).unwrap();
        for part in [
            PreparedPart::Text {
                text: "document text".into(),
            },
            PreparedPart::Image { image },
        ] {
            // A busy native session must keep even a caller-disconnected request admitted.
            let session_lock = host.active_session.lock().await;
            let lease = host.media_admission.try_acquire().unwrap();
            let request = ChatEnvelope {
                messages: vec![cluaiz_core::ChatMessage {
                    role: "user".into(),
                    content: vec![part],
                }],
                samplers: Default::default(),
                think_mode: "off".into(),
                reasoning_effort: Default::default(),
                reasoning_budget_message: None,
                chat_template_kwargs: None,
                grammar: None,
                response_format: None,
            };
            let response = host
                .dispatch_prepared_with_progress(
                    request,
                    1,
                    Path::new("not-loaded.gguf"),
                    &ActiveUserSettings::default(),
                    None,
                    None,
                    None,
                    Some(lease.clone()),
                    None,
                )
                .await;
            drop(lease);
            assert!(host.media_admission.try_acquire().is_err());
            let HostResponse::TokenStream(mut receiver) = response else {
                panic!("Expected worker stream")
            };
            drop(session_lock);
            let error = receiver.recv().await.unwrap();
            assert_eq!(
                super::super::stream_error(&error),
                Some("No model loaded in EngineHost")
            );
            while receiver.recv().await.is_some() {}
            assert!(host.media_admission.try_acquire().is_ok());
        }
    }
}
