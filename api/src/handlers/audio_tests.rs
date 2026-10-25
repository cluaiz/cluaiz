//! Shared audio selection/options tests; excluded from production builds.
use super::*;
fn request() -> AudioExecuteRequest {
    AudioExecuteRequest {
        model: None,
        task: None,
        instruction: None,
        input_source: InputSource {
            source_type: "text".into(),
            data: "sample".into(),
        },
        parameters: None,
        keep_alive: None,
    }
}
#[test]
fn test_unsupported_audio_options_are_errors() {
    let mut request = request();
    assert!(validate_options(&request, AudioTask::TextToSpeech).is_ok());
    request.parameters = Some(AudioParameters {
        voice_id: Some("hf_alpha".into()),
        speed: Some(1.5),
        ..Default::default()
    });
    let options = validate_options(&request, AudioTask::TextToSpeech).unwrap();
    assert_eq!(
        (options.voice.as_deref(), options.speed),
        (Some("hf_alpha"), Some(1.5))
    );
    assert!(validate_options(&request, AudioTask::SpeechToText).is_err());
    request.parameters = Some(AudioParameters {
        voice_id: Some("default".into()),
        ..Default::default()
    });
    assert!(validate_options(&request, AudioTask::TextToSpeech)
        .unwrap()
        .voice
        .is_none());
    request.parameters = Some(AudioParameters {
        speed: Some(9.0),
        ..Default::default()
    });
    assert!(validate_options(&request, AudioTask::TextToSpeech).is_err());
    // The text's language for TTS; the speaker's languages only for STT
    request.parameters = Some(AudioParameters {
        language: Some("cs".into()),
        ..Default::default()
    });
    assert_eq!(
        validate_options(&request, AudioTask::TextToSpeech)
            .unwrap()
            .language
            .as_deref(),
        Some("cs")
    );
    request.parameters = Some(AudioParameters {
        language: Some("not a code!".into()),
        ..Default::default()
    });
    assert!(validate_options(&request, AudioTask::TextToSpeech).is_err());
    request.parameters = Some(AudioParameters {
        languages: vec!["hi".into()],
        ..Default::default()
    });
    assert!(validate_options(&request, AudioTask::TextToSpeech).is_err());
    assert!(validate_options(&request, AudioTask::SpeechToText).is_ok());
    request.parameters = Some(AudioParameters {
        timestamps: Some(true),
        ..Default::default()
    });
    assert!(validate_options(&request, AudioTask::SpeechToText).is_err());
    request.parameters = None;
    request.keep_alive = Some(-2);
    assert!(validate_options(&request, AudioTask::SpeechToText).is_err());
}
#[test]
fn test_explicit_missing_model_is_not_replaced() {
    let entry:ModelRegistryEntry=serde_json::from_value(json!({
        "id":"model-long","category":"tts","format_type":"ONNX","huggingface_repo":"",
        "local_dir":"","files":[],"extra_files":null,"supported_tasks":["text-to-speech"],"requires_gpu":false,
        "metadata":{"architecture":"test","parameters":"","context_window":"",
        "quantization":null,"bit_depth":null,"chat_template":null}
    })).expect("Test registry fixture");
    let mut registry = ModelRegistry::default();
    registry.installed_models.insert(entry.id.clone(), entry);
    assert_eq!(
        select_model(&registry, None, None, AudioTask::TextToSpeech)
            .expect("Declared TTS model")
            .id,
        "model-long"
    );
    assert!(select_model(
        &registry,
        Some("model"),
        Some("other"),
        AudioTask::TextToSpeech
    )
    .is_err());
}
