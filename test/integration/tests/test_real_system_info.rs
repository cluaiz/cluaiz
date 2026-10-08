use engine_core::hardware::governor::HardwareGovernor;
use engine_core::hardware::system_info::HardwareOrchestrator;

#[test]
fn test_system_info_resolution() {
    let system_info = HardwareOrchestrator::probe_live();
    assert!(!system_info.silicon_truth.cpu.brand.is_empty(), "CPU brand must be detected");
    assert!(system_info.silicon_truth.cpu.physical_cores > 0, "CPU physical cores must be > 0");
    assert!(system_info.silicon_truth.memory.total_capacity_gb > 0.0, "Total RAM must be > 0");

    let cal_res = HardwareGovernor::auto_calibrate();
    assert!(cal_res.is_ok(), "Auto calibration must succeed: {:?}", cal_res.err());

    let loaded = HardwareGovernor::load_system_info().expect("Failed to load saved system_info");
    assert_eq!(loaded.identity.os_target, system_info.identity.os_target);
    assert_eq!(loaded.silicon_truth.cpu.brand, system_info.silicon_truth.cpu.brand);
}

#[test]
fn test_compare_execution_latency_decision() {
    let system_info = HardwareOrchestrator::probe_live();
    
    // 1. Small model test (1.96 GB, N = 1) -> Fits in GPU VRAM
    let small_model = HardwareOrchestrator::compare_execution_latency(1.96, 1, &system_info);
    assert!(small_model.cpu_latency_ms > 0.0);
    assert_eq!(small_model.candidates.len(), 5, "Must evaluate all 5 llama.cpp execution candidates");
    
    if !system_info.silicon_truth.accelerators.gpus.is_empty() {
        assert!(small_model.model_fits_in_vram, "1.96 GB model must fit in GPU VRAM");
        assert_eq!(small_model.winner_candidate_id, "candidate_a_full_gpu", "Candidate A (Full GPU) must win for 1.96 GB model");
        assert!(small_model.winner_generation_tps > small_model.candidates[3].generation_tps, "GPU TPS must beat CPU TPS");
        assert!(small_model.speedup_ratio > 1.0, "GPU speedup ratio over CPU must be > 1.0");
        assert!(small_model.gpu_vram_latency_ms.is_some());
    }

    // 2. Large model test (11.96 GB, N = 1) -> Exceeds GPU VRAM
    let large_model = HardwareOrchestrator::compare_execution_latency(11.96, 1, &system_info);
    assert!(large_model.cpu_latency_ms > 0.0);
    assert_eq!(large_model.candidates.len(), 5);
    if !system_info.silicon_truth.accelerators.gpus.is_empty() {
        assert!(!large_model.model_fits_in_vram, "11.96 GB model cannot fit fully in 4GB VRAM");
        assert!(!large_model.candidates[0].feasible, "Candidate A must be infeasible for 11.96 GB model on 4GB GPU");
    }
}

#[test]
fn test_all_installed_models_real_execution_plans() {
    let installed_models = vec![
        "bonsai-4b-gguf-q1_0",
        "bonsai-27b-gguf-q1_0",
        "bonsai1-8b",
        "gemma-4-26b-a4b-it-qat-gguf-UD-Q4_K_XL",
        "gemma-4-e4b-it-gguf-Q4_K_M",
        "llama_3.2_instruct-3b-gguf-q4_k_m",
        "qwen3-4b-gguf-Q4_K_M",
    ];

    let mut output_log = String::new();
    output_log.push_str("================================================================================\n");
    output_log.push_str("CLUAIZ REAL HARDWARE EXECUTION PLAN REPORT (LIVE MODEL REGISTRY TRUTH)\n");
    output_log.push_str("================================================================================\n\n");

    for model_id in &installed_models {
        println!("\n🔍 Testing Real Hardware Plan for: {}", model_id);
        let req = engine_core::hardware::resource_negotiator::ExecutionPlanRequest {
            model: Some(model_id.to_string()),
            ..Default::default()
        };

        let plan = engine_core::hardware::resource_negotiator::plan_execution_candidates(&req)
            .unwrap_or_else(|e| panic!("Failed to plan candidate execution for model '{}': {}", model_id, e));

        assert_eq!(plan.candidates.len(), 5, "Must evaluate all 5 llama.cpp candidates for {}", model_id);
        assert!(plan.architecture.is_some() && !plan.architecture.as_ref().unwrap().is_empty(), "Architecture must be probed for {}", model_id);
        assert!(plan.max_input_tokens > 0, "max_input_tokens must be positive");
        assert!(plan.max_output_tokens > 0, "max_output_tokens must be positive");
        assert!(!plan.supported_inputs.is_empty(), "supported_inputs must not be empty");
        assert!(!plan.supported_outputs.is_empty(), "supported_outputs must not be empty");
        assert!(!plan.supported_attachments.is_empty(), "supported_attachments must not be empty");

        // Verify Clean Unified Schema Presence
        assert!(!plan.model_info.model_id.is_empty(), "model_id must be populated for {}", model_id);
        assert!(!plan.model_info.architecture.is_empty(), "architecture must be populated for {}", model_id);
        assert!(plan.hardware_info.ram.total_ram_gb > 0.0, "RAM info must be populated");
        assert!(plan.hardware_info.storage_ssd.speed_mbps > 0.0, "Storage SSD speed must be populated");
        assert!(plan.active_user_settings.vram_buffer_gb > 0.0, "Active user settings VRAM buffer must be > 0");
        assert!(!plan.active_user_settings.kv_cache_quantization.is_empty(), "KV cache quantization must be populated");

        // Models that fit in physical available memory must have a valid feasible placement
        if plan.weights_gb <= (plan.hardware_info.gpu.usable_vram_gb + plan.hardware_info.ram.usable_ram_gb) {
            assert_ne!(plan.execution_plan.placement_mode, "infeasible", "Model {} must be feasible", model_id);
            assert!(plan.execution_plan.gpu_layers > 0 || plan.execution_plan.ram_layers > 0);
        } else {
            assert_eq!(plan.execution_plan.placement_mode, "infeasible", "Oversized model {} must be marked infeasible", model_id);
        }

        // Format JSON
        let plan_json = serde_json::to_string_pretty(&plan).expect("Failed to serialize execution plan to JSON");
        
        output_log.push_str(&format!("### MODEL: {}\n", model_id));
        output_log.push_str(&format!("  Architecture: {:?}\n", plan.architecture));
        output_log.push_str(&format!("  Context Window: {}\n", plan.n_ctx));
        output_log.push_str(&format!("  KV Cache GB: {:.3}\n", plan.kv_cache_gb));
        output_log.push_str(&format!("  Compute Buffer GB: {:.3}\n", plan.compute_buffer_gb));
        output_log.push_str(&format!("  Winner Candidate: {}\n", plan.winner_candidate_id));
        output_log.push_str(&format!("  Winner TPS: {:.2}\n", plan.winner_generation_tps));
        output_log.push_str(&format!("  Winner TTFT ms: {:.2}\n", plan.winner_ttft_ms));
        output_log.push_str(&format!("  Winner GPU Layers: {} / {}\n", plan.gpu_layers, plan.gpu_layers + plan.cpu_layers));
        output_log.push_str(&format!("  Decision: {}\n", plan.decision_reason));
        output_log.push_str("--------------------------------------------------------------------------------\n");
        output_log.push_str("CANDIDATES BREAKDOWN:\n");
        for c in &plan.candidates {
            output_log.push_str(&format!(
                "  [{}] feasible={} | vram={:.2}GB | ram={:.2}GB | kv_loc={} | tps={:.2} | ttft={:.1}ms | is_winner={}{}\n",
                c.id, c.feasible, c.vram_required_gb, c.ram_required_gb, c.kv_location, c.generation_tps, c.ttft_ms, c.is_winner,
                if let Some(ref r) = c.ineligibility_reason { format!(" | reason: {}", r) } else { String::new() }
            ));
        }
        output_log.push_str("\nFULL JSON RESPONSE:\n");
        output_log.push_str(&plan_json);
        output_log.push_str("\n\n================================================================================\n\n");

        println!("  ✅ Architecture: {:?}", plan.architecture);
        println!("  ✅ Winner: {} (TPS: {:.2}, TTFT: {:.2}ms)", plan.winner_candidate_id, plan.winner_generation_tps, plan.winner_ttft_ms);
    }

    // Also test uninstalled model
    let uninstalled_req = engine_core::hardware::resource_negotiator::ExecutionPlanRequest {
        model: Some("qwen2.5-7b-instruct-q4_k_m".to_string()),
        ..Default::default()
    };
    let uninstalled_res = engine_core::hardware::resource_negotiator::plan_execution_candidates(&uninstalled_req);
    assert!(uninstalled_res.is_err(), "Uninstalled model must return error");
    let err_msg = uninstalled_res.err().unwrap();
    output_log.push_str("### UNINSTALLED MODEL: qwen2.5-7b-instruct-q4_k_m\n");
    output_log.push_str(&format!("  Expected Error: {}\n", err_msg));
    output_log.push_str("================================================================================\n");

    // Write full output to text file in workspace
    let dump_path = std::path::Path::new("c:/Users/Aryan/my/Cluaiz-workspace/live_model_plans_test_output.txt");
    std::fs::write(dump_path, &output_log).expect("Failed to write live_model_plans_test_output.txt");
    println!("\n📄 Saved comprehensive test output to: {:?}", dump_path);
}

#[test]
fn test_uninstalled_model_returns_clean_error() {
    let req = engine_core::hardware::resource_negotiator::ExecutionPlanRequest {
        model: Some("qwen2.5-7b-instruct-q4_k_m".to_string()),
        ..Default::default()
    };
    let res = engine_core::hardware::resource_negotiator::plan_execution_candidates(&req);
    assert!(res.is_err(), "Uninstalled model must return clean error instead of fake 1.0 GB data");
}


