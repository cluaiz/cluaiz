//! Real-router task client for audio acceptance; no audio/inference response is fabricated.
use anyhow::{bail, Context, Result};
use axum::Router;
use serde_json::{json, Value};

pub async fn run(router: &Router, params: Value) -> Result<std::result::Result<Value, Value>> {
    Ok(run_with_events(router, params).await?.0)
}

pub async fn run_with_events(
    router: &Router,
    params: Value,
) -> Result<(std::result::Result<Value, Value>, Value)> {
    let task = match crate::support::cep(router, "audio.run", params).await? {
        Ok(task) => task,
        Err(error) => return Ok((Err(error), Value::Null)),
    };
    let id = task["task"]["task_id"].as_str().context("Audio task ID")?;
    const MAX_AUDIO_TEST_SECONDS: u64 = 180;
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(MAX_AUDIO_TEST_SECONDS);
    loop {
        let result = crate::support::cep(
            router,
            "tasks.result",
            json!({"task_id":id,"wait_ms":30000}),
        )
        .await?;
        if matches!(&result, Err(error) if error["code"] == -32009) {
            if std::time::Instant::now() >= deadline {
                let _ = crate::support::cep(router, "tasks.cancel", json!({"task_id":id})).await;
                bail!("Audio task {id} exceeded the test deadline");
            }
            continue;
        }
        let status = crate::support::cep(router, "tasks.get", json!({"task_id":id}))
            .await?
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        return Ok((result, status));
    }
}
