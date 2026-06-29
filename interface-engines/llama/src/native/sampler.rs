use crate::ffi::llama_cpp;
use cluaiz_shared::StructuralDNA;
use tracing::info;

/// 🎲 Builds a dynamic sampler chain based on model DNA (handles BitNet 1-bit logic natively).
pub unsafe fn build_sampler_chain(
    dna: &StructuralDNA,
    tokens: &[i32],
) -> anyhow::Result<*mut std::ffi::c_void> {
    let sparams = llama_cpp::LlamaSamplerChainParams { no_perf: true };
    let sampler_chain = llama_cpp::llama_sampler_chain_init(sparams);
    
    if sampler_chain.is_null() {
        return Err(anyhow::anyhow!("💀 Failed to initialize sampler chain"));
    }

    if !dna.signature.is_bitnet {
        let temp = dna.inference_params.get("temperature").and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.7);
        let top_p = dna.inference_params.get("top_p").and_then(|p| p.parse::<f32>().ok()).unwrap_or(0.95);
        let repeat_last_n = dna.inference_params.get("repeat_last_n").and_then(|n| n.parse::<i32>().ok()).unwrap_or(64);
        let repeat_penalty = dna.inference_params.get("repeat_penalty").and_then(|p| p.parse::<f32>().ok()).unwrap_or(1.1);
        
        llama_cpp::llama_sampler_chain_add(
            sampler_chain,
            llama_cpp::llama_sampler_init_penalties(
                repeat_last_n,
                repeat_penalty,
                0.0, // frequency penalty
                0.0, // presence penalty
            )
        );

        if temp <= 0.0 {
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_greedy());
            info!("🎲 [Native-Llama] Temperature is zero: Forcing Greedy Sampler.");
        } else {
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_top_p(top_p, 1));
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_temp(temp));
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(1234);
            llama_cpp::llama_sampler_chain_add(sampler_chain, llama_cpp::llama_sampler_init_dist(seed));
            info!("🎲 [Native-Llama] Dynamic Sampler (Top-P -> Temp -> Dist): temp={}, top_p={}, repeat_penalty={}, seed={}", temp, top_p, repeat_penalty, seed);
        }
    } else {
        let repeat_last_n = dna.inference_params.get("repeat_last_n").and_then(|n| n.parse::<i32>().ok()).unwrap_or(64);
        let repeat_penalty = dna.inference_params.get("repeat_penalty").and_then(|p| p.parse::<f32>().ok()).unwrap_or(1.1);
        
        llama_cpp::llama_sampler_chain_add(
            sampler_chain,
            llama_cpp::llama_sampler_init_penalties(
                repeat_last_n,
                repeat_penalty,
                0.0,
                0.0,
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
