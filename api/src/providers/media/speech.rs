use cluaiz_core::providers::Target;
use serde_json::{json, Value};

use super::{ignored_of, provider_error, send_to, with_headers};

/// `/v1/audio/speech` for a provider's model. The provider gets the OpenAI request (`input`,
/// `voice`, `speed`, WAV audio) and the answer comes back in the Engine's own shape: `output.
/// audio_data` as a `data:audio/wav;base64,…` URI, or with `stream`, one `speech.audio.delta`
/// event and the `done` event, so an app reads both kinds of model the same way.
pub async fn forward_speech(target: Target, request: Value) -> axum::response::Response {
    if let Err(message) = target.check_kind("tts") {
        return provider_error(&target, 400, message);
    }
    let ignored = ignored_of(&request, &["model", "input", "voice", "speed", "stream"]);
    let mut body = json!({ "model": target.model, "input": request.get("input").cloned().unwrap_or(Value::Null), "response_format": "wav" });
    for field in ["voice", "speed"] {
        if let Some(value) = request.get(field).filter(|v| !v.is_null()) {
            body[field] = value.clone();
        }
    }
    let response = match send_to(
        &target,
        "/audio/speech",
        body.to_string().into(),
        "application/json",
    )
    .await
    {
        Ok(response) => response,
        Err(reply) => return reply,
    };
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
        .unwrap_or_default();
    let audio = match response.bytes().await {
        Ok(audio) => audio,
        Err(error) => {
            return provider_error(
                &target,
                502,
                format!("{}: the audio did not arrive: {error}", target.provider),
            )
        }
    };
    use base64::Engine as _;
    let (audio, media_type) = if audio.starts_with(b"RIFF") {
        (wav_with_sizes(&audio), "audio/wav".to_string())
    } else {
        // Not WAV after all: labelled with what the provider says it is
        (
            audio.to_vec(),
            if media_type.starts_with("audio/") {
                media_type
            } else {
                "application/octet-stream".into()
            },
        )
    };
    let uri = format!(
        "data:{media_type};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&audio)
    );
    let result = json!({ "status": "success", "task": "text_to_speech", "model": target.full_name(), "info": null, "output": { "text": null, "audio_data": uri, "segments": null } });
    if request
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let format = if media_type == "audio/wav" {
            "wav"
        } else {
            media_type.trim_start_matches("audio/")
        };
        let delta = json!({ "status": "chunk", "type": "speech.audio.delta", "index": 0, "format": format, "audio": result["output"]["audio_data"] });
        let done = json!({ "status": "done", "result": result });
        let text = format!("data: {delta}\n\ndata: {done}\n\n");
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

/// A WAV with its RIFF and `data` sizes set from its real length. Providers that stream audio
/// write 0xFFFFFFFF there (the length is unknown when the header goes out), which players and the
/// Engine's own decoder read as a truncated file.
fn wav_with_sizes(wav: &[u8]) -> Vec<u8> {
    let mut out = wav.to_vec();
    if out.len() < 12 || &out[0..4] != b"RIFF" || &out[8..12] != b"WAVE" {
        return out;
    }
    let riff = u32::try_from(out.len() - 8).unwrap_or(u32::MAX);
    out[4..8].copy_from_slice(&riff.to_le_bytes());
    let mut at = 12;
    while at + 8 <= out.len() {
        let size =
            u32::from_le_bytes([out[at + 4], out[at + 5], out[at + 6], out[at + 7]]) as usize;
        if &out[at..at + 4] == b"data" {
            let real = u32::try_from(out.len() - at - 8).unwrap_or(u32::MAX);
            if size > real as usize {
                out[at + 4..at + 8].copy_from_slice(&real.to_le_bytes());
            }
            break;
        }
        // Chunks are padded to an even length
        at += 8 + size + (size & 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_streamed_wav_gets_its_real_sizes() {
        let mut wav = b"RIFF\xff\xff\xff\xffWAVEfmt \x10\x00\x00\x00".to_vec();
        wav.extend_from_slice(&[0u8; 16]);
        wav.extend_from_slice(b"LIST\x02\x00\x00\x00ab");
        wav.extend_from_slice(b"data\xff\xff\xff\xff");
        wav.extend_from_slice(&[1u8, 2, 3, 4]);
        let fixed = wav_with_sizes(&wav);
        assert_eq!(
            u32::from_le_bytes(fixed[4..8].try_into().unwrap()) as usize,
            fixed.len() - 8
        );
        let data = fixed.len() - 12;
        assert_eq!(&fixed[data..data + 4], b"data");
        assert_eq!(
            u32::from_le_bytes(fixed[data + 4..data + 8].try_into().unwrap()),
            4
        );
        assert_eq!(wav_with_sizes(b"ID3 not a wav"), b"ID3 not a wav".to_vec());
    }
}
