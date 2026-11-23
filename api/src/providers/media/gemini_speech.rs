use base64::Engine as _;
use cluaiz_core::{media_limits::MediaLimits, providers::Target, Settings};
use serde_json::{json, Value};

use super::{provider_error, send_to_url};

/// Translate only the official Gemini Developer API base. A catalogue URL never receives a key.
pub(in crate::providers) fn endpoint(target: &Target) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(&target.config.base_url).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("generativelanguage.googleapis.com")
        || !matches!(
            url.path().trim_end_matches('/'),
            "/v1beta/openai" | "/v1beta"
        )
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.path_segments_mut().ok()?.clear().extend([
        "v1beta",
        "models",
        &format!("{}:generateContent", target.model),
    ]);
    Some(url)
}

pub(super) async fn generate(
    target: &Target,
    endpoint: &reqwest::Url,
    input: &str,
    voice: Option<&str>,
) -> Result<Vec<u8>, axum::response::Response> {
    let mut config = json!({ "responseModalities": ["AUDIO"] });
    if let Some(voice) = voice {
        config["speechConfig"] =
            json!({ "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } });
    }
    let body =
        json!({ "contents": [{ "parts": [{ "text": input }] }], "generationConfig": config });
    let mut response = send_to_url(
        target,
        endpoint.as_str(),
        body.to_string().into(),
        "application/json",
        Some("x-goog-api-key"),
    )
    .await?;
    let limits = Settings::current().media.limits;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| provider_error(target, 502, "Could not read Gemini speech JSON".into()))?
    {
        limits
            .check_input_bytes((bytes.len() as u64).saturating_add(chunk.len() as u64))
            .map_err(|error| provider_error(target, 502, error.to_string()))?;
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| provider_error(target, 502, "Gemini returned invalid speech JSON".into()))?;
    let result = value
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .and_then(|parts| parts.iter().find_map(|part| part.get("inlineData")))
        .ok_or_else(|| provider_error(target, 502, "Gemini returned no speech audio".into()))?;
    decode(result, limits).map_err(|message| provider_error(target, 502, message))
}

fn decode(data: &Value, limits: MediaLimits) -> Result<Vec<u8>, String> {
    let data_string = data
        .get("data")
        .and_then(Value::as_str)
        .ok_or("Missing Gemini audio data")?;
    limits
        .check_input_bytes(data_string.len() as u64)
        .map_err(|error| error.to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_string)
        .map_err(|_| "Gemini returned invalid base64 audio")?;
    if bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        return Ok(bytes);
    }
    let mime = data
        .get("mimeType")
        .and_then(Value::as_str)
        .ok_or("Missing Gemini audio format")?;
    let mut fields = mime.split(';').map(str::trim);
    if !fields
        .next()
        .is_some_and(|value| value.eq_ignore_ascii_case("audio/l16"))
        || bytes.is_empty()
        || bytes.len() % 2 != 0
    {
        return Err("Gemini returned an unsupported audio format".into());
    }
    let rate = fields
        .find_map(|field| field.strip_prefix("rate="))
        .ok_or("Missing Gemini sample rate")?
        .parse::<u32>()
        .map_err(|_| "Invalid Gemini sample rate")?;
    limits
        .check_decoded_bytes((bytes.len() as u64).saturating_mul(4))
        .map_err(|error| error.to_string())?;
    let samples: Vec<f32> = bytes
        .chunks_exact(2)
        .map(|sample| f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32768.0)
        .collect();
    cluaiz_media::wav::encode_pcm16(&samples, rate, limits).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_core::providers::Provider;

    #[test]
    fn test_gemini_speech_endpoint_uses_official_host_only() {
        let target = |base| Target {
            provider: "demo".into(),
            config: Provider::new(base),
            model: "speech-demo".into(),
        };
        let url = endpoint(&target(
            "https://generativelanguage.googleapis.com/v1beta/openai",
        ));
        assert_eq!(
            url.as_ref().map(reqwest::Url::path),
            Some("/v1beta/models/speech-demo:generateContent")
        );
        assert!(endpoint(&target("https://example.com/v1beta/openai")).is_none());
    }

    #[test]
    fn test_gemini_pcm_response_becomes_wav() -> Result<(), String> {
        let limits = MediaLimits::default();
        let payload = json!({ "data": "AQD/fw==", "mimeType": "audio/L16;codec=pcm;rate=24000" });
        let wav = decode(&payload, limits)?;
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[24..28], &24_000u32.to_le_bytes());
        assert!(decode(
            &json!({ "data": "?", "mimeType": "audio/L16;rate=24000" }),
            limits
        )
        .is_err());
        Ok(())
    }
}
