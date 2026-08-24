use crate::ffi::llama_cpp;
use cluaiz_shared::StructuralDNA;
use tracing::info;

/// 🎲 Builds a dynamic sampler chain based on model DNA (handles BitNet 1-bit logic natively).
pub unsafe fn build_sampler_chain(
    dna: &StructuralDNA,
    tokens: &[i32],
    req_samplers: Option<&serde_json::Value>,
) -> anyhow::Result<*mut std::ffi::c_void> {
    let sparams = llama_cpp::LlamaSamplerChainParams { no_perf: true };
    let sampler_chain = llama_cpp::llama_sampler_chain_init(sparams);
    
    if sampler_chain.is_null() {
        return Err(anyhow::anyhow!("💀 Failed to initialize sampler chain"));
    }

    let req_temp = req_samplers.and_then(|s| s.get("temp")).and_then(|v| v.as_f64()).map(|t| t as f32);
    let req_top_p = req_samplers.and_then(|s| s.get("top_p")).and_then(|v| v.as_f64()).map(|p| p as f32);
    let req_top_k = req_samplers.and_then(|s| s.get("top_k")).and_then(|v| v.as_i64()).map(|k| k as i32);
    let req_min_p = req_samplers.and_then(|s| s.get("min_p")).and_then(|v| v.as_f64()).map(|mp| mp as f32);
    let req_presence_penalty = req_samplers.and_then(|s| s.get("presence_penalty")).and_then(|v| v.as_f64()).map(|p| p as f32).unwrap_or(0.0);
    let req_repeat_penalty = req_samplers.and_then(|s| s.get("repeat_penalty")).and_then(|v| v.as_f64()).map(|p| p as f32);

    if !dna.signature.is_bitnet {
        let temp = req_temp
            .or_else(|| dna.inference_params.get("temperature").and_then(|t| t.parse::<f32>().ok()))
            .unwrap_or(0.7);
        let top_p = req_top_p
            .or_else(|| dna.inference_params.get("top_p").and_then(|p| p.parse::<f32>().ok()))
            .unwrap_or(0.95);
        let top_k = req_top_k
            .or_else(|| dna.inference_params.get("top_k").and_then(|k| k.parse::<i32>().ok()))
            .unwrap_or(40);
        let min_p = req_min_p
            .or_else(|| dna.inference_params.get("min_p").and_then(|mp| mp.parse::<f32>().ok()))
            .unwrap_or(0.05);
        let repeat_last_n = dna.inference_params.get("repeat_last_n").and_then(|n| n.parse::<i32>().ok()).unwrap_or(64);
        let repeat_penalty = req_repeat_penalty
            .or_else(|| dna.inference_params.get("repeat_penalty").and_then(|p| p.parse::<f32>().ok()))
            .unwrap_or(1.1);
        
        llama_cpp::llama_sampler_chain_add(
            sampler_chain,
            llama_cpp::llama_sampler_init_penalties(
                repeat_last_n,
                repeat_penalty,
                0.0, // frequency penalty
                req_presence_penalty, // presence penalty
            )
        );

        if temp <= 0.0 {
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_greedy());
            info!("🎲 [Native-Llama] Temperature is zero ({:.2}): Forcing Greedy Sampler.", temp);
        } else {
            if top_k > 0 {
                llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_top_k(top_k));
            }
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_top_p(top_p, 1));
            if min_p > 0.0 {
                llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_min_p(min_p, 1));
            }
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_temp(temp));
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(1234);
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_dist(seed));
            info!("🎲 [Native-Llama] Dynamic Sampler Chain (Penalties -> Top-K({}) -> Top-P({}) -> Min-P({}) -> Temp({}) -> Dist): seed={}", top_k, top_p, min_p, temp, seed);
        }
    } else {
        let repeat_last_n = dna.inference_params.get("repeat_last_n").and_then(|n| n.parse::<i32>().ok()).unwrap_or(64);
        let repeat_penalty = req_repeat_penalty
            .or_else(|| dna.inference_params.get("repeat_penalty").and_then(|p| p.parse::<f32>().ok()))
            .unwrap_or(1.1);
        
        llama_cpp::llama_sampler_chain_add(
            sampler_chain,
            llama_cpp::llama_sampler_init_penalties(
                repeat_last_n,
                repeat_penalty,
                0.0,
                req_presence_penalty,
            )
        );

        llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_greedy());
        info!("🎲 [Native-Llama] 1-Bit Model Detected: Forcing Greedy-Only Sampler with Repetition Penalty.");
    }

    for &token in tokens {
        llama_cpp::llama_sampler_accept(sampler_chain, token);
    }

    Ok(sampler_chain)
}
