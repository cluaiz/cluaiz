pub mod slots;

/// Shared filesystem authorization for every attachment consumer. Remote/inline data
/// has no client-selected local file; temporary source lifetime belongs to cluaiz-media.
/// Standard routes execute as the authenticated owner; client-supplied caller/approval
/// headers do not define a principal. CEP methods check their own `Call` principal.
pub(crate) async fn authorize_source(
    state: &std::sync::Arc<crate::AppState>,
    _headers: &axum::http::HeaderMap,
    source: &mut String,
) -> Result<Option<std::path::PathBuf>, (&'static str, String)> {
    let Some(path) = cluaiz_media::source::local_source_path(source)
        .map_err(|error| ("invalid_request_error", error.to_string()))?
    else {
        return Ok(None);
    };
    let workspace = state.current_workspace_dir.read().await.clone();
    let path = path
        .to_str()
        .ok_or_else(|| ("invalid_request_error", "Media path must be UTF-8".into()))?;
    let canonical = crate::cep::roots::resolve(path, &workspace, false)
        .map_err(|error| ("invalid_request_error", error.to_string()))?;
    *source = canonical
        .to_str()
        .ok_or_else(|| ("invalid_request_error", "Media path must be UTF-8".into()))?
        .to_owned();
    Ok(Some(canonical))
}

/// Actual shared request policy/preparation owner; consumers import it directly.
pub(crate) async fn prepare_authorized_media(
    state: &std::sync::Arc<crate::AppState>,
    headers: &axum::http::HeaderMap,
    mut request: cluaiz_core::ChatEnvelope<Vec<cluaiz_core::media::InputPart>>,
    settings: &cluaiz_core::ActiveUserSettings,
    document_images: bool,
    cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    progress: Option<cluaiz_core::processing::ProcessingReporter>,
) -> Result<cluaiz_core::ChatEnvelope<Vec<cluaiz_core::media::PreparedPart>>, (&'static str, String)>
{
    use cluaiz_core::media::InputPart;
    let mut allowed = std::collections::HashSet::new();
    for part in request
        .messages
        .iter_mut()
        .flat_map(|message| &mut message.content)
    {
        let source = match part {
            InputPart::Image { source }
            | InputPart::Audio { source }
            | InputPart::Document { source }
            | InputPart::Video { source, .. } => source,
            _ => continue,
        };
        if let Some(canonical) = authorize_source(state, headers, source).await? {
            allowed.insert(canonical);
        }
    }
    let mut resolver = cluaiz_media::source::SourceResolver::from_environment(settings.media)
        .map_err(|error| ("invalid_request_error", error.to_string()))?
        .with_document_images(document_images);
    if let Some(signal) = cancellation {
        resolver = resolver.with_cancellation(signal);
    }
    resolver = resolver.with_progress(progress);
    cluaiz_media::request::prepare_chat(request, &resolver, |path| {
        if allowed.contains(path) {
            Ok(())
        } else {
            anyhow::bail!("Media file was not authorized for this request")
        }
    })
    .await
    .map_err(|error| ("invalid_request_error", format!("{error:#}")))
}

/// Prepare one authorized audio source for STT or a speech speaker reference.
pub(crate) async fn prepare_authorized_audio(
    state: &std::sync::Arc<crate::AppState>,
    headers: &axum::http::HeaderMap,
    source: String,
    settings: &cluaiz_core::ActiveUserSettings,
) -> Result<cluaiz_core::media::DecodedAudio, (&'static str, String)> {
    use cluaiz_core::{
        media::{InputPart, PreparedPart},
        ChatEnvelope, ChatMessage,
    };
    let request = ChatEnvelope {
        messages: vec![ChatMessage {
            role: "user".into(),
            content: vec![InputPart::Audio { source }],
        }],
        samplers: Default::default(),
        think_mode: "off".into(),
        reasoning_effort: Default::default(),
        reasoning_budget_message: None,
        chat_template_kwargs: None,
        grammar: None,
        response_format: None,
    };
    let prepared =
        prepare_authorized_media(state, headers, request, settings, false, None, None).await?;
    match prepared.messages.into_iter().flat_map(|m| m.content).next() {
        Some(PreparedPart::Audio { audio }) => Ok(audio),
        _ => Err(("invalid_input", "Audio source did not produce PCM".into())),
    }
}

/// Shared task error envelope; preserve local-file approval details for clients.
pub(crate) fn task_error(
    status: axum::http::StatusCode,
    kind: &str,
    message: impl ToString,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use serde_json::{json, Value};
    let message = message.to_string();
    let details = if kind == "permission_error" {
        serde_json::from_str::<Value>(&message).ok()
    } else {
        None
    };
    (status, axum::Json(json!({"status":"error", "error":{"type":kind,
        "message":details.as_ref().and_then(|d|d.get("message")).and_then(Value::as_str).unwrap_or(&message),
        "details":details}}))).into_response()
}

#[cfg(test)]
mod tests {
    use super::authorize_source;
    use axum::http::HeaderMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_standard_local_source_uses_owner_and_shared_canonical_path() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("owner.md");
        std::fs::write(&file, "owner attachment").unwrap();
        let state = Arc::new(crate::AppState::new_test_state(workspace.path().to_owned()));
        state.permission_schema.write().await.agent_security_mode = "strict".into();
        let mut headers = HeaderMap::new();
        headers.insert("x-caller", "agent".parse().unwrap());
        headers.insert("x-approval-id", "untrusted-header".parse().unwrap());
        let mut source = file.to_string_lossy().to_string();
        let canonical = authorize_source(&state, &headers, &mut source)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(canonical, std::fs::canonicalize(file).unwrap());
        assert_eq!(source, canonical.to_str().unwrap());
    }

    #[tokio::test]
    async fn test_standard_relative_missing_and_inline_sources_share_one_boundary() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("notes.md"), "notes").unwrap();
        let state = Arc::new(crate::AppState::new_test_state(workspace.path().to_owned()));
        let headers = HeaderMap::new();
        let mut relative = "notes.md".to_owned();
        let resolved = authorize_source(&state, &headers, &mut relative)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            resolved,
            std::fs::canonicalize(workspace.path().join("notes.md")).unwrap()
        );
        let mut missing = "missing.md".to_owned();
        assert_eq!(
            authorize_source(&state, &headers, &mut missing)
                .await
                .unwrap_err()
                .0,
            "invalid_request_error"
        );
        let mut inline = "data:text/plain;base64,bm90ZXM=".to_owned();
        let before = inline.clone();
        assert!(authorize_source(&state, &headers, &mut inline)
            .await
            .unwrap()
            .is_none());
        assert_eq!(inline, before);
    }
}
