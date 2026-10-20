//! Minimal GGUF v3 fixtures for metadata tests, never compiled into the library/CLI.

use std::path::{Path, PathBuf};

pub(crate) fn asset_manifest() -> serde_json::Value {
    serde_json::json!({
        "model_family":"audio8_tts", "activation_dtype":"float16",
        "slow_logits_layout":"semantic_then_eos", "kv_attention_layout":"valid_prefix",
        "default_precision":"int4", "available_precisions":["int4"],
        "default_codec_precision":"fp16", "codec_models":{"fp16":"codec_decoder_fp16.onnx"}
    })
}

/// Signature-only protobuf fixture; never a claimed inference result.
pub(crate) fn onnx(path: &Path, input: &str, output: &str, external: Option<&str>) {
    fn varint(mut value: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        while value >= 128 {
            bytes.push((value as u8 & 127) | 128);
            value >>= 7;
        }
        bytes.push(value as u8);
        bytes
    }
    fn message(number: u64, value: &[u8]) -> Vec<u8> {
        let mut bytes = varint((number << 3) | 2);
        bytes.extend(varint(value.len() as u64));
        bytes.extend(value);
        bytes
    }
    fn value_info(name: &str) -> Vec<u8> {
        let shape = message(1, &[8, 1]); // Dimension.dim_value = 1.
        let mut tensor = vec![8, 1];
        tensor.extend(message(2, &shape)); // FLOAT[1].
        let mut value = message(1, name.as_bytes());
        value.extend(message(2, &message(1, &tensor)));
        value
    }
    let mut graph = message(11, &value_info(input));
    graph.extend(message(12, &value_info(output)));
    if let Some(location) = external {
        let mut pair = message(1, b"location");
        pair.extend(message(2, location.as_bytes()));
        let mut tensor = message(8, b"weight");
        tensor.extend(message(13, &pair));
        graph.extend(message(5, &tensor));
    }
    std::fs::write(path, message(7, &graph)).unwrap();
}

fn string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend((value.len() as u64).to_le_bytes());
    bytes.extend(value.as_bytes());
}

pub(crate) fn gguf(
    path: &Path,
    values: &[(&str, &str)],
    booleans: &[(&str, bool)],
    tensor: Option<&str>,
) {
    let mut bytes = b"GGUF".to_vec();
    bytes.extend(3_u32.to_le_bytes());
    bytes.extend(u64::from(tensor.is_some()).to_le_bytes());
    bytes.extend(((values.len() + booleans.len()) as u64).to_le_bytes());
    for &(key, value) in values {
        string(&mut bytes, key);
        bytes.extend(8_u32.to_le_bytes());
        string(&mut bytes, value);
    }
    for &(key, value) in booleans {
        string(&mut bytes, key);
        bytes.extend(7_u32.to_le_bytes());
        bytes.push(u8::from(value));
    }
    if let Some(name) = tensor {
        string(&mut bytes, name);
        bytes.extend(1_u32.to_le_bytes());
        bytes.extend(1_u64.to_le_bytes());
        bytes.extend(0_u32.to_le_bytes());
        bytes.extend(0_u64.to_le_bytes());
        bytes.resize(bytes.len().div_ceil(32) * 32, 0);
        bytes.extend(0_f32.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

pub(crate) fn primary(dir: &Path) -> PathBuf {
    let path = dir.join("main.gguf");
    gguf(
        &path,
        &[
            ("general.architecture", "opaque"),
            ("opaque.embedding_length", "32"),
            ("tokenizer.ggml.model", "vocabulary"),
        ],
        &[],
        Some("blk.0.attn_v.weight"),
    );
    path
}

pub(crate) fn config(dir: &Path) {
    std::fs::write(
        dir.join("config.json"),
        r#"{"vision_config":{"hidden_size":32},"audio_config":{"hidden_size":32}}"#,
    )
    .unwrap();
}

pub(crate) fn projector(path: &Path, dimension: &str, audio: bool) {
    gguf(
        path,
        &[
            ("general.architecture", "clip"),
            ("clip.projector_type", "linear"),
            ("clip.vision.projection_dim", dimension),
        ],
        &[
            ("clip.has_vision_encoder", true),
            ("clip.has_audio_encoder", audio),
        ],
        None,
    );
}
