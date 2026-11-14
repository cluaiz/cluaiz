//! The ONNX Runtime environment of this process. ort allows one per process and builds every
//! session on it (`Session::builder(environment()?)`), so it is made once, on first use, and kept.

use ort::environment::Environment;
use std::sync::OnceLock;

static ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();

/// The process's ONNX Runtime environment; the error says why ONNX Runtime could not start
pub fn environment() -> anyhow::Result<&'static Environment> {
    ENVIRONMENT
        .get_or_init(|| ort::init().with_name("cluaiz_onnx_env").build().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| anyhow::anyhow!("ONNX Runtime could not start: {error}"))
}
