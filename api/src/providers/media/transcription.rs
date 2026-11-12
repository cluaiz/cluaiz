use cluaiz_core::providers::Target;
use serde_json::{json, Value};

use super::{ignored_of, provider_error, send_to, with_headers};

/// The multipart body of an OpenAI transcription request: the audio as `audio.wav` and the text
/// fields. Returns the body and its boundary.
fn multipart(fields: &[(&str, String)], wav: &[u8]) -> (Vec<u8>, String) {
    let boundary = format!("cluaiz-{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::with_capacity(wav.len() + 512);
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (body, boundary)
}

/// `/v1/audio/transcriptions` for a provider's model. The audio is read as for an installed model
/// (`input_source` or `file`: a data URI, a URL or an allowed local file, any format the Engine
/// decodes), sent to the provider as WAV with `language`, `prompt` and `temperature`, and the text
/// comes back in the Engine's own shape (`output.text`, with `segments` when `timestamps` is on).
pub async fn forward_transcription(
    state: std::sync::Arc<crate::AppState>,
    target: Target,
    request: Value,
) -> axum::response::Response {
    if let Err(message) = target.check_kind("stt") {
        return provider_error(&target, 400, message);
    }
    let ignored = ignored_of(
        &request,
        &[
            "model",
            "file",
            "input_source",
            "prompt",
            "language",
            "temperature",
            "timestamps",
            "stream",
        ],
    );
    let source = match (
        request
            .pointer("/input_source/data")
            .and_then(Value::as_str),
        request.get("file").and_then(Value::as_str),
    ) {
        (Some(data), _) if !data.trim().is_empty() => {
            let kind = request
                .pointer("/input_source/source_type")
                .and_then(Value::as_str)
                .unwrap_or("audio")
                .to_ascii_lowercase();
            if kind == "base64" && !data.starts_with("data:") {
                format!("data:application/octet-stream;base64,{data}")
            } else {
                data.to_string()
            }
        }
        (_, Some(file)) if !file.trim().is_empty() => file.to_string(),
        _ => {
            return provider_error(
                &target,
                400,
                "send the audio in `input_source` or `file`".into(),
            )
        }
    };
    let settings = cluaiz_core::Settings::current();
    let audio = match crate::utils::prepare_authorized_audio(&state, source, &settings, None).await
    {
        Ok(audio) => audio,
        Err((kind, message)) => {
            return provider_error(
                &target,
                if kind == "permission_error" { 403 } else { 400 },
                message,
            )
        }
    };
    // The provider gets one channel: the channels' mean
    let channels = audio.channels().max(1) as usize;
    let mono: Vec<f32> = audio
        .samples()
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    let wav =
        match cluaiz_media::wav::encode_pcm16(&mono, audio.sample_rate(), settings.media.limits) {
            Ok(wav) => wav,
            Err(error) => {
                return provider_error(
                    &target,
                    400,
                    format!("the audio could not be prepared: {error}"),
                )
            }
        };
    let timestamps = request
        .get("timestamps")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut fields = vec![
        ("model", target.model.clone()),
        (
            "response_format",
            if timestamps { "verbose_json" } else { "json" }.to_string(),
        ),
    ];
    for field in ["language", "prompt"] {
        if let Some(value) = request
            .get(field)
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty() && *v != "auto")
        {
            fields.push((field, value.to_string()));
        }
    }
    if let Some(temperature) = request.get("temperature").and_then(Value::as_f64) {
        fields.push(("temperature", temperature.to_string()));
    }
    if timestamps {
        fields.push(("timestamp_granularities[]", "segment".into()));
    }
    let (body, boundary) = multipart(&fields, &wav);
    let response = match send_to(
        &target,
        "/audio/transcriptions",
        body.into(),
        &format!("multipart/form-data; boundary={boundary}"),
    )
    .await
    {
        Ok(response) => response,
        Err(reply) => return reply,
    };
    let answer: Value = match response.json().await {
        Ok(answer) => answer,
        Err(error) => {
            return provider_error(
                &target,
                502,
                format!(
                    "{}: the transcription is not JSON: {error}",
                    target.provider
                ),
            )
        }
    };
    let segments = answer.get("segments").and_then(Value::as_array).map(|segments| {
        segments.iter().map(|s| json!({ "start": s["start"], "end": s["end"], "text": s["text"].as_str().map(str::trim).unwrap_or_default(), "speaker": null })).collect::<Vec<_>>()
    });
    let text = answer
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let result = json!({ "status": "success", "task": "speech_to_text", "model": target.full_name(), "info": null, "output": { "text": text, "audio_data": null, "segments": segments } });
    if request
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        // The provider answers at once: the whole text is the one event
        let text = format!(
            "data: {}\n\n",
            json!({ "status": "done", "result": result })
        );
        return with_headers(
            axum::response::Response::builder()
                .status(200)
                .header(axum::http::header::CONTENT_TYPE, "text/event-stream"),
            &target,
            &ignored,
        )
        .body(axum::body::Body::from(text))
        .unwrap_or_else(|_| {
            provider_error(
                &target,
                500,
                "the provider's answer could not be passed on".into(),
            )
        });
    }
    with_headers(
        axum::response::Response::builder()
            .status(200)
            .header(axum::http::header::CONTENT_TYPE, "application/json"),
        &target,
        &ignored,
    )
    .body(axum::body::Body::from(result.to_string()))
    .unwrap_or_else(|_| {
        provider_error(
            &target,
            500,
            "the provider's answer could not be passed on".into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transcription_request_is_multipart_with_the_audio_last() {
        let (body, boundary) = multipart(
            &[("model", "whisper".into()), ("language", "en".into())],
            b"RIFFdata",
        );
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper\r\n"
        )));
        assert!(text.contains(
            "name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFdata\r\n"
        ));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
        assert_eq!(
            ignored_of(
                &json!({ "model": "m", "input": "x", "keep_alive": 5, "user": null }),
                &["model", "input"]
            ),
            vec!["keep_alive"]
        );
    }
}
