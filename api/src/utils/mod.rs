pub mod slots;

/// Shared filesystem authorization for every attachment consumer. Remote/inline data
/// has no client-selected local file; temporary source lifetime belongs to cluaiz-media.
pub(crate) async fn authorize_source(
    state: &std::sync::Arc<crate::AppState>,
    headers: &axum::http::HeaderMap,
    source: &mut String,
) -> Result<Option<std::path::PathBuf>, (&'static str, String)> {
    use crate::handlers::fs_jail::{check_permission_gate, resolve_workspace_path};
    let Some(path) = cluaiz_media::source::local_source_path(source)
        .map_err(|error| ("invalid_request_error", error.to_string()))?
    else {
        return Ok(None);
    };
    let workspace = state.current_workspace_dir.read().await.clone();
    let path = path
        .to_str()
        .ok_or_else(|| ("invalid_request_error", "Media path must be UTF-8".into()))?;
    let verification = resolve_workspace_path(path, &workspace, false)
        .map_err(|(_, error)| ("invalid_request_error", error.to_string()))?;
    check_permission_gate(
        state,
        "fs_read",
        &verification,
        source,
        headers
            .get("x-caller")
            .and_then(|value| value.to_str().ok()),
        None,
        headers
            .get("x-approval-id")
            .and_then(|value| value.to_str().ok()),
    )
    .await
    .map_err(|(_, error)| ("permission_error", error.to_string()))?;
    let canonical = verification.resolved_path().to_path_buf();
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
    cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
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
        .map_err(|error| ("invalid_request_error", error.to_string()))?;
    if let Some(signal) = cancellation {
        resolver = resolver.with_cancellation(signal);
    }
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
