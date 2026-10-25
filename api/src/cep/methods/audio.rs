//! `audio.*: voices, languages and audio tasks` (03-method-catalog Part B). Owner: Wave B (05-chatgpt-plan.md §2).

use crate::{
    cep::{roots, CallCtx},
    handlers::audio,
};
use cluaiz_cep::{Call, CepError, Meta, Registry, Scope, TaskHandle};
use cluaiz_runtime::onnx_host::{AudioEvent, AudioTask, OnnxModel};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CatalogParams {
    model: Option<String>,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "audio.voices",
        Meta::new(
            Scope::Read,
            "Voice inventory and speech capabilities without loading weights",
        )
        .example(
            json!({}),
            json!({"object":"list","model":"installed-model","data":[]}),
        ),
        voices,
    );
    registry.add(
        "audio.languages",
        Meta::new(
            Scope::Read,
            "Languages declared by the selected transcription model",
        )
        .example(
            json!({}),
            json!({"object":"list","model":"installed-model","data":[]}),
        ),
        languages,
    );
    registry.add(
        "audio.run",
        Meta::new(
            Scope::Read,
            "Run the existing speech/transcription executor; chunks are task events",
        )
        .task()
        .example(
            json!({"input_source":{"type":"text","data":"Speech input"}}),
            json!({"task":{"task_id":"t_example","status":"working"}}),
        ),
        run,
    );
}

fn contract_error(error: impl ToString) -> CepError {
    let mut error = roots::invalid("model", &error.to_string());
    if let Some(data) = error.data.as_mut() {
        data["type"] = json!("model_contract_error");
    }
    error
}

async fn voices(ctx: CallCtx, call: Call, params: CatalogParams) -> Result<Value, CepError> {
    let entry =
        match audio::selected_model(&ctx.state, params.model.as_deref(), AudioTask::TextToSpeech)
            .await
        {
            Ok(entry) => entry,
            Err(error) => return Err(error.into_cep().await),
        };
    audio::authorize_model(&ctx.state, &call, &entry).await?;
    let mut data = vec![json!({"id":"default","language":null,"default":true})];
    if entry.format_type.eq_ignore_ascii_case("gguf") {
        let capabilities =
            cluaiz_runtime::speech_host::capabilities(&entry).map_err(contract_error)?;
        return Ok(
            json!({"object":"list","model":entry.id,"data":data,"capabilities":capabilities}),
        );
    }
    let model =
        OnnxModel::from_registry(&entry, AudioTask::TextToSpeech).map_err(contract_error)?;
    data.extend(
        model
            .voices()
            .into_iter()
            .map(|voice| json!({"id":voice.id,"language":voice.language,"default":false})),
    );
    let capabilities = model.tts_capabilities().map_err(contract_error)?;
    Ok(json!({"object":"list","model":model.id,"data":data,"capabilities":capabilities}))
}

async fn languages(ctx: CallCtx, call: Call, params: CatalogParams) -> Result<Value, CepError> {
    let entry =
        match audio::selected_model(&ctx.state, params.model.as_deref(), AudioTask::SpeechToText)
            .await
        {
            Ok(entry) => entry,
            Err(error) => return Err(error.into_cep().await),
        };
    audio::authorize_model(&ctx.state, &call, &entry).await?;
    let model =
        OnnxModel::from_registry(&entry, AudioTask::SpeechToText).map_err(contract_error)?;
    Ok(json!({"object":"list","model":model.id,"data":model.stt_languages()}))
}

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// TaskHandle exposes a cooperative flag; poll it without blocking the request/runtime.
async fn cancelled(handle: &TaskHandle) {
    const CANCEL_POLL_MS: u64 = 25;
    while !handle.is_cancelled() {
        tokio::time::sleep(std::time::Duration::from_millis(CANCEL_POLL_MS)).await;
    }
}

async fn run(
    ctx: CallCtx,
    call: Call,
    params: audio::AudioExecuteRequest,
) -> Result<Value, CepError> {
    // The same call (identity, digest, approval) when the work outlives the request
    let task_call = call.clone();
    call.start_task(move |handle| async move {
        let cancellation = Arc::new(AtomicBool::new(false));
        let _cancel_on_drop = CancelOnDrop(Arc::clone(&cancellation));
        let cancel = cancelled(&handle);
        tokio::pin!(cancel);
        let prepared = tokio::select! {
            _ = &mut cancel => return Err(CepError::cancelled()),
            result = audio::dispatch_audio(&ctx.state, axum::http::HeaderMap::new(), params,
                Some(&task_call), cancellation) => result,
        };
        let ready = match prepared {
            Ok(ready) => ready,
            Err(error) => return Err(error.into_cep().await),
        };
        collect_events(ready, &handle, cancel).await
    })
}

async fn collect_events<F: std::future::Future<Output = ()>>(
    mut ready: audio::DispatchedAudio,
    handle: &TaskHandle,
    cancel: F,
) -> Result<Value, CepError> {
    tokio::pin!(cancel);
    let mut index = 0_u64;
    loop {
        let event = tokio::select! {
            _ = &mut cancel => return Err(CepError::cancelled()),
            event = ready.events.recv() => event,
        };
        match event {
            Some(Ok(AudioEvent::Chunk(chunk))) => {
                let event = if ready.task == AudioTask::TextToSpeech {
                    json!({"type":"speech.audio.delta","index":index,"format":"wav","audio":chunk})
                } else {
                    json!({"type":"transcript.delta","index":index,"text":chunk})
                };
                handle.event(event);
                index += 1;
            }
            Some(Ok(AudioEvent::Completed(output))) => {
                return Ok(json!({
                "model":ready.model,"task":ready.task.as_str(),"output":audio::audio_output(output)}))
            }
            Some(Err(message)) => return Err(CepError::unavailable("engine_error", message)),
            None => {
                return Err(CepError::unavailable(
                    "engine_error",
                    "Audio worker ended without a result",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_cep::TaskStore;
    use cluaiz_runtime::onnx_host::AudioOutput;
    use tokio::sync::mpsc;

    // Synthetic worker events exercise only the adapter, not native model inference.
    fn task(
        events: mpsc::Receiver<Result<AudioEvent, String>>,
        kind: AudioTask,
    ) -> (Arc<TaskStore>, String) {
        let store = Arc::new(TaskStore::new(2, 60_000, 120_000));
        let view = store
            .start("audio-test", "audio.run", None, |handle| async move {
                let cancel = cancelled(&handle);
                collect_events(
                    audio::DispatchedAudio {
                        events,
                        task: kind,
                        model: "test-worker".into(),
                    },
                    &handle,
                    cancel,
                )
                .await
            })
            .unwrap();
        let id = view["task_id"].as_str().unwrap().to_owned();
        (store, id)
    }

    #[tokio::test]
    async fn test_audio_events_and_final_domain_output() {
        for (kind, output, event_type, field) in [
            (
                AudioTask::SpeechToText,
                AudioOutput::Transcript("adapter result".into()),
                "transcript.delta",
                "text",
            ),
            (
                AudioTask::TextToSpeech,
                AudioOutput::WavDataUri("adapter-only".into()),
                "speech.audio.delta",
                "audio",
            ),
        ] {
            let (tx, rx) = mpsc::channel(2);
            tx.send(Ok(AudioEvent::Chunk("adapter chunk".into())))
                .await
                .unwrap();
            tx.send(Ok(AudioEvent::Completed(output))).await.unwrap();
            let (store, id) = task(rx, kind);
            let result = store.result("audio-test", &id, 5_000).await.unwrap();
            assert_eq!(result["task"], kind.as_str());
            assert!(result.get("status").is_none() && result.get("info").is_none());
            assert!(result["output"].is_object());
            let view = store.get("audio-test", &id, None).unwrap();
            assert_eq!(view["status"], "completed");
            assert_eq!(view["events"][0]["type"], event_type);
            assert_eq!(view["events"][0][field], "adapter chunk");
            assert_eq!(view["events"][0]["index"], 0);
        }
    }

    #[tokio::test]
    async fn test_worker_failure_or_missing_completion_is_not_success() {
        for worker_error in [None, Some("native worker failed")] {
            let (tx, rx) = mpsc::channel(1);
            if let Some(message) = worker_error {
                tx.send(Err(message.into())).await.unwrap();
            }
            drop(tx);
            let (store, id) = task(rx, AudioTask::SpeechToText);
            let error = store.result("audio-test", &id, 5_000).await.unwrap_err();
            assert_eq!(error.kind(), Some("engine_error"));
            assert_eq!(
                store.get("audio-test", &id, None).unwrap()["status"],
                "failed"
            );
        }
    }

    #[tokio::test]
    async fn test_cancellation_closes_worker_receiver_and_sets_preparation_flag() {
        let (tx, rx) = mpsc::channel(1);
        let (store, id) = task(rx, AudioTask::SpeechToText);
        store.cancel("audio-test", &id).unwrap();
        assert_eq!(
            store
                .result("audio-test", &id, 5_000)
                .await
                .unwrap_err()
                .kind(),
            Some("cancelled")
        );
        tokio::time::timeout(std::time::Duration::from_secs(2), tx.closed())
            .await
            .unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        drop(CancelOnDrop(flag.clone()));
        assert!(flag.load(Ordering::Relaxed));
    }
}
