use crate::ffi::llama_cpp;
use cluaiz_models::types::StructuralDNA;
use tracing::info;

/// 🎲 Builds a dynamic sampler chain based on model DNA (handles BitNet 1-bit logic natively).
pub unsafe fn build_sampler_chain(
    dna: &StructuralDNA,
    tokens: &[i32],
    req_samplers: Option<&serde_json::Value>,
    n_vocab: i32,
) -> anyhow::Result<*mut std::ffi::c_void> {
    let sparams = llama_cpp::LlamaSamplerChainParams { no_perf: true };
    let sampler_chain = llama_cpp::llama_sampler_chain_init(sparams);
    
    if sampler_chain.is_null() {
        return Err(anyhow::anyhow!("💀 Failed to initialize sampler chain"));
    }

    // Order: chat request value -> user default (settings) -> model DNA -> llama.cpp default
    let user = cluaiz_core::Settings::current().samplers;
    let req_f32 = |k: &str| req_samplers.and_then(|s| s.get(k)).and_then(|v| v.as_f64()).map(|v| v as f32);
    let req_temp = req_f32("temp").or(user.temp);
    let req_top_p = req_f32("top_p").or(user.top_p);
    let req_top_k = req_samplers.and_then(|s| s.get("top_k")).and_then(|v| v.as_i64()).map(|k| k as i32).or(user.top_k);
    let req_min_p = req_f32("min_p").or(user.min_p);
    let req_presence_penalty = req_f32("presence_penalty").or(user.presence_penalty).unwrap_or(0.0);
    let req_frequency_penalty = req_f32("frequency_penalty").or(user.frequency_penalty).unwrap_or(0.0);
    let req_repeat_penalty = req_f32("repeat_penalty").or(user.repeat_penalty);
    let req_seed = req_samplers.and_then(|s| s.get("seed")).and_then(|v| v.as_u64()).map(|s| s as u32).or(user.seed);

    if !dna.signature.is_bitnet {
        let temp = req_temp
            .or_else(|| dna.inference_params.get("temperature").and_then(|t: &String| t.parse::<f32>().ok()))
            .unwrap_or(0.8); // llama.cpp default
        let top_p = req_top_p
            .or_else(|| dna.inference_params.get("top_p").and_then(|p: &String| p.parse::<f32>().ok()))
            .unwrap_or(0.95);
        let top_k = req_top_k
            .or_else(|| dna.inference_params.get("top_k").and_then(|k: &String| k.parse::<i32>().ok()))
            .unwrap_or(40);
        let min_p = req_min_p
            .or_else(|| dna.inference_params.get("min_p").and_then(|mp: &String| mp.parse::<f32>().ok()))
            .unwrap_or(0.05);
        let repeat_last_n = dna.inference_params.get("repeat_last_n").and_then(|n: &String| n.parse::<i32>().ok()).unwrap_or(64);
        let repeat_penalty = req_repeat_penalty
            .or_else(|| dna.inference_params.get("repeat_penalty").and_then(|p: &String| p.parse::<f32>().ok()))
            .unwrap_or(1.0); // llama.cpp default (1.0 = off)
        
        llama_cpp::llama_sampler_chain_add(
            sampler_chain,
            llama_cpp::llama_sampler_init_penalties(
                n_vocab,
                repeat_last_n,
                repeat_penalty,
                req_frequency_penalty,
                req_presence_penalty,
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
            let seed = req_seed.unwrap_or_else(|| {
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(1234)
            });
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_dist(seed));
            info!("🎲 [Native-Llama] Dynamic Sampler Chain (Penalties -> Top-K({}) -> Top-P({}) -> Min-P({}) -> Temp({}) -> Dist): seed={}", top_k, top_p, min_p, temp, seed);
        }
    } else {
        let repeat_last_n = dna.inference_params.get("repeat_last_n").and_then(|n: &String| n.parse::<i32>().ok()).unwrap_or(64);
        let repeat_penalty = req_repeat_penalty
            .or_else(|| dna.inference_params.get("repeat_penalty").and_then(|p: &String| p.parse::<f32>().ok()))
            .unwrap_or(1.1);
        
        llama_cpp::llama_sampler_chain_add(
            sampler_chain,
            llama_cpp::llama_sampler_init_penalties(
                n_vocab,
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
