//! What a finished chat turn reports and cleans up, shared by the streaming and non-streaming answers

use super::preflight::PreparedChatContext;
use super::tool_turns::ToolTurns;
use crate::state::AppState;
use cluaiz_runtime::ContextReport;
use serde_json::{json, Value};
use std::sync::Arc;

/// The engine's own counts for one turn (every generation step, tool calls included)
pub struct TurnCounts {
    pub completion_tokens: usize,
    pub reasoning_tokens: usize,
    pub first_token_ms: u128,
}

/// The tools active in this turn: the session's and the request's
pub fn active_tool_ids(ctx: &PreparedChatContext, session_id: Option<&str>) -> Vec<String> {
    let mut ids = session_id.map(cluaiz_agents::ToolsEngine::get_active_tool_ids_for_session).unwrap_or_default();
    for id in &ctx.active_tool_ids {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids
}

/// Prompt tokens: the engine's count of the last generation, else a text estimate
fn prompt_tokens(ctx: &PreparedChatContext, report: Option<&ContextReport>) -> usize {
    match report {
        Some(report) => report.prompt_tokens,
        None => estimate_tokens(ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars),
    }
}

/// About four characters per token, used only when the engine has no count (no generation ran)
fn estimate_tokens(chars: usize) -> usize {
    if chars > 0 { (chars / 4).max(1) } else { 0 }
}

/// The standard OpenAI fields and the timings, from counts already known
fn base_usage(prompt: usize, counts: &TurnCounts, total_time_ms: u128, model_load_ms: u128) -> Value {
    let completion = counts.completion_tokens;
    // Speed over the decoding time; very short answers fall back to the whole time
    let decode_time_ms = total_time_ms.saturating_sub(counts.first_token_ms);
    let seconds = if decode_time_ms > 30 { decode_time_ms } else { total_time_ms } as f64 / 1000.0;
    let tokens_per_second = if seconds > 0.0 { (completion as f64 / seconds * 100.0).round() / 100.0 } else { 0.0 };
    json!({
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "total_tokens": prompt + completion,
        "completion_tokens_details": { "reasoning_tokens": counts.reasoning_tokens },
        "time_to_first_token_ms": counts.first_token_ms,
        "total_time_ms": total_time_ms,
        "model_load_ms": model_load_ms,
        "tokens_per_second": tokens_per_second
    })
}

/// OpenAI `usage` plus timings. The context breakdown and loaded models are added only with the
/// owner's stream telemetry setting (`ctx.send_telemetry`).
pub fn usage(ctx: &PreparedChatContext, session_id: Option<&str>, report: Option<&ContextReport>, counts: &TurnCounts) -> Value {
    let completion = counts.completion_tokens;
    let mut usage = base_usage(prompt_tokens(ctx, report), counts, ctx.start_time.elapsed().as_millis(), ctx.model_load_ms);
    if !ctx.send_telemetry {
        return usage;
    }
    let mut telemetry = cluaiz_agents::ToolsEngine::compute_telemetry(
        &ctx.resolved_model_name,
        session_id.unwrap_or("ephemeral"),
        &active_tool_ids(ctx, session_id),
        ctx.user_prompt_chars,
        ctx.history_chars,
        ctx.system_prompt_chars,
        completion,
    );
    // Totals from the engine's own count of what the context holds now
    if let Some(report) = report {
        telemetry.context_breakdown.apply_measured(report.n_ctx, report.n_tokens);
    }
    usage["context_telemetry"] = serde_json::to_value(telemetry).unwrap_or_default();
    if cluaiz_security::PermissionSchema::load().model_header_info {
        usage["model_header_info"] = json!({ "active_models": super::types::generate_model_header_info() });
    }
    usage
}

/// Ends the turn's tools: counts down the session's tool turns and unloads the ones not used
pub fn end_tools(ctx: &PreparedChatContext, session_id: Option<&str>, turns: &ToolTurns) {
    if let Some(sid) = session_id {
        cluaiz_agents::ToolsEngine::decrement_session_turns(sid);
    }
    super::tool_turns::end_turn_tools(ctx, session_id, turns);
}

/// `keep_alive` after the answer: 0 unloads the model now, N minutes schedules the unload
pub async fn apply_keep_alive(state: &Arc<AppState>, keep_alive: Option<i32>) {
    match keep_alive {
        Some(0) => {
            tracing::info!("[Memory] Unloading the model after the answer (keep_alive: 0)");
            state.host.unload_model().await;
        }
        Some(mins) if mins > 0 => {
            let secs = (mins as u64) * 60;
            let state = Arc::clone(state);
            tracing::info!("[Memory] Unloading the model in {mins} minutes (keep_alive)");
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                tracing::info!("[Memory] Unloading the model after keep_alive ({mins}m)");
                state.host.unload_model().await;
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(completion: usize, reasoning: usize, first_token_ms: u128) -> TurnCounts {
        TurnCounts { completion_tokens: completion, reasoning_tokens: reasoning, first_token_ms }
    }

    #[test]
    fn usage_has_the_openai_fields_and_totals() {
        let usage = base_usage(42, &counts(380, 128, 200), 2_200, 15);
        assert_eq!(usage["prompt_tokens"], 42);
        assert_eq!(usage["completion_tokens"], 380);
        assert_eq!(usage["total_tokens"], 422);
        assert_eq!(usage["completion_tokens_details"]["reasoning_tokens"], 128);
        assert_eq!(usage["time_to_first_token_ms"], 200);
        assert_eq!(usage["total_time_ms"], 2_200);
        assert_eq!(usage["model_load_ms"], 15);
        // 380 tokens over the 2.0 s after the first token
        assert_eq!(usage["tokens_per_second"], 190.0);
    }

    #[test]
    fn speed_of_a_very_short_answer_uses_the_whole_time() {
        // decode time 20 ms (under 30): 10 tokens over the whole 0.5 s
        let usage = base_usage(5, &counts(10, 0, 480), 500, 0);
        assert_eq!(usage["tokens_per_second"], 20.0);
    }

    #[test]
    fn no_time_and_no_tokens_give_zero_not_a_division_error() {
        let usage = base_usage(0, &counts(0, 0, 0), 0, 0);
        assert_eq!(usage["tokens_per_second"], 0.0);
        assert_eq!(usage["total_tokens"], 0);
    }

    #[test]
    fn prompt_estimate_only_when_there_is_text() {
        assert_eq!(estimate_tokens(0), 0);
        assert_eq!(estimate_tokens(3), 1);
        assert_eq!(estimate_tokens(400), 100);
    }
}
