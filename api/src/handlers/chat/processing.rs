//! Opt-in progress multiplexing begins before preflight, so source decoding is observable live.
//! Authorization and preparation remain in their existing owners; token strings never carry events.

use super::{
    agent_loop,
    preflight::PreparedChatContext,
    types::{ExternalChatRequest, StreamSignals, ACTIVE_STREAMS, DEFAULT_MAX_TOKENS},
};
use crate::AppState;
use axum::{
    body::{Body, Bytes},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use cluaiz_core::processing::{
    ProcessingEvent, ProcessingReporter, ProcessingStage, ProcessingState,
};
use futures::StreamExt;
use std::sync::{atomic::Ordering, Arc};

const PROGRESS_QUEUE_CAPACITY: usize = 128;
const PROGRESS_HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(5);

struct RequestGuard {
    id: String,
    progress: ProcessingReporter,
    prepare: Option<tokio::task::AbortHandle>,
}
impl Drop for RequestGuard {
    fn drop(&mut self) {
        if let Some(task) = &self.prepare {
            task.abort();
        }
        self.progress.cancellation().store(true, Ordering::Release);
        if let Ok(mut streams) = ACTIVE_STREAMS.write() {
            streams.remove(&self.id);
        }
    }
}

fn event_bytes(id: &str, event: ProcessingEvent) -> Bytes {
    let value = serde_json::json!({"id":id, "object":"cluaiz.processing", "processing":event});
    Bytes::from(format!("event: cluaiz.progress\ndata: {value}\n\n"))
}

pub(super) async fn stream_with_progress(
    state: Arc<AppState>,
    headers: HeaderMap,
    request: ExternalChatRequest,
    id: String,
) -> Response {
    // Bounded, best-effort observations: a slow UI cannot grow memory or block native work.
    let (sender, mut receiver) = tokio::sync::mpsc::channel(PROGRESS_QUEUE_CAPACITY);
    let progress = ProcessingReporter::new(move |event| {
        let _ = sender.try_send(event);
    });
    let stream = async_stream::stream! {
        let mut guard = RequestGuard { id: id.clone(), progress: progress.clone(), prepare: None };
        if let Ok(mut streams) = ACTIVE_STREAMS.write() {
            streams.insert(id.clone(), StreamSignals { cancel: progress.cancellation(), ..Default::default() });
        }
        let mut terminal_sent = false;
        progress.emit(ProcessingStage::Queued, ProcessingState::Running, None, None);
        let prepare_id = id.clone();
        let prepare_progress = progress.clone();
        // Preflight contains synchronous native/metadata calls. Polling it inside the HTTP body
        // can delay flushing an already-yielded event; its owned task keeps transport polling separate.
        let mut prepare = tokio::spawn(async move {
            let ctx = match PreparedChatContext::prepare(&state, &request, &prepare_id, &headers, Some(prepare_progress.clone())).await {
                Ok(ctx) => ctx,
                Err(response) => {
                    prepare_progress.finish(if prepare_progress.cancelled() { ProcessingState::Cancelled } else { ProcessingState::Failed });
                    return response;
                }
            };
            let dispatch = ctx.dispatch(&state.host, None, ctx.validated_max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)).await;
            agent_loop::execute_streaming_loop(state.clone(), request, ctx, dispatch).await
        });
        guard.prepare = Some(prepare.abort_handle());
        let mut heartbeat = tokio::time::interval(PROGRESS_HEARTBEAT);
        heartbeat.tick().await;
        let response = loop {
            tokio::select! {
                biased;
                Some(event) = receiver.recv() => {
                    if !terminal_sent {
                        terminal_sent = event.stage == ProcessingStage::Request;
                        yield Ok::<_, axum::Error>(event_bytes(&id, event));
                    }
                },
                response = &mut prepare => match response {
                    Ok(response) => break response,
                    Err(error) => {
                        tracing::error!(%error, "Chat preparation task failed");
                        progress.finish(ProcessingState::Failed);
                        if !terminal_sent {
                            if let Some(event) = progress.terminal_event() { yield Ok(event_bytes(&id, event)); }
                        }
                        yield Ok(Bytes::from(format!("data: {}\n\ndata: [DONE]\n\n", serde_json::json!({
                            "id":id, "error":{"message":"Chat preparation task failed", "type":"engine_error"}
                        }))));
                        return;
                    }
                },
                _ = heartbeat.tick() => yield Ok(Bytes::from_static(b": keep-alive\n\n")),
            }
        };
        let mut body = response.into_body().into_data_stream();
        loop {
            tokio::select! {
                biased;
                Some(event) = receiver.recv() => {
                    if !terminal_sent {
                        terminal_sent = event.stage == ProcessingStage::Request;
                        yield Ok(event_bytes(&id, event));
                    }
                },
                item = body.next() => match item {
                    Some(item) => {
                        // Polling the body can produce both an observation and [DONE] at once.
                        while let Ok(event) = receiver.try_recv() {
                            if !terminal_sent {
                                terminal_sent = event.stage == ProcessingStage::Request;
                                yield Ok(event_bytes(&id, event));
                            }
                        }
                        if !terminal_sent {
                            if let Some(event) = progress.terminal_event() {
                                terminal_sent = true;
                                yield Ok(event_bytes(&id, event));
                            }
                        }
                        yield item;
                    },
                    None => break,
                },
                _ = heartbeat.tick() => yield Ok(Bytes::from_static(b": keep-alive\n\n")),
            }
        }
        while let Ok(event) = receiver.try_recv() {
            if !terminal_sent {
                terminal_sent = event.stage == ProcessingStage::Request;
                yield Ok(event_bytes(&id, event));
            }
        }
    };
    (
        [
            (axum::http::header::CONTENT_TYPE, "text/event-stream"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dropped_progress_stream_cancels_its_preflight_task() {
        let progress = ProcessingReporter::new(|_| {});
        let task = tokio::spawn(std::future::pending::<()>());
        let guard = RequestGuard {
            id: uuid::Uuid::new_v4().to_string(),
            progress: progress.clone(),
            prepare: Some(task.abort_handle()),
        };
        drop(guard);
        assert!(progress.cancelled());
        assert!(task.await.unwrap_err().is_cancelled());
    }
}
