use color_eyre::Result;
use colored::Colorize;
use cluaiz_shared::hardware::governor::HardwareGovernor;
use cluaiz_shared::hardware::schema::booster::{
    BoosterMode, KvCacheQuantization, ContextShiftingMode, FeatureState
};

pub async fn execute(
    kv_quant: Option<String>,
    context_shift: Option<String>,
    mode: Option<String>,
    spec_decode: Option<String>,
) -> Result<()> {
    let mut control = HardwareGovernor::load_booster_settings().unwrap_or_default();
    let mut modified = false;

    // Check if any arguments were provided
    let has_args = kv_quant.is_some() || context_shift.is_some() || mode.is_some() || spec_decode.is_some();

    if has_args {
        if let Some(m) = mode {
            control.mode_run = match m.to_lowercase().as_str() {
                "edge" => BoosterMode::Edge,
                "multitasking" => BoosterMode::Multitasking,
                "balance" => BoosterMode::Balance,
                "max_boost" => BoosterMode::MaxBoost,
                "ultra_max_boost" => BoosterMode::UltraMaxBoost,
                "hyper_cluster" => BoosterMode::HyperCluster,
                _ => {
                    println!("⚠️  Invalid mode '{}'. Keeping current value.", m);
                    control.mode_run
                }
            };
            modified = true;
        }

        if let Some(kv) = kv_quant {
            control.kv_cache_quantization = match kv.to_lowercase().as_str() {
                "auto" => KvCacheQuantization::Auto,
                "kv16" => KvCacheQuantization::Kv16,
                "kv8" => KvCacheQuantization::Kv8,
                "kv4" => KvCacheQuantization::Kv4,
                _ => {
                    println!("⚠️  Invalid KV quantization '{}'. Keeping current value.", kv);
                    control.kv_cache_quantization
                }
            };
            modified = true;
        }

        if let Some(cs) = context_shift {
            control.context_shifting = match cs.to_lowercase().as_str() {
                "auto" => ContextShiftingMode::Auto,
                "off" => ContextShiftingMode::Off,
                "minimal" => ContextShiftingMode::Minimal,
                "standard" => ContextShiftingMode::Standard,
                "aggressive" => ContextShiftingMode::Aggressive,
                "extreme" => ContextShiftingMode::Extreme,
                _ => {
                    println!("⚠️  Invalid context shifting mode '{}'. Keeping current value.", cs);
                    control.context_shifting
                }
            };
            modified = true;
        }

        if let Some(sd) = spec_decode {
            control.speculative_decoding = match sd.to_lowercase().as_str() {
                "auto" => FeatureState::Auto,
                "on" => FeatureState::On,
                "off" => FeatureState::Off,
                _ => {
                    println!("⚠️  Invalid speculative decoding mode '{}'. Keeping current value.", sd);
                    control.speculative_decoding
                }
            };
            modified = true;
        }
    } else {
        // Loop-based Interactive configuration
        println!("\n  {} {}", "🚀".cyan(), "cluaiz Booster - Interactive Performance Setup".bold());
        
        loop {
            println!("\n  {} {}", "📊".cyan(), "Current Booster Settings:".bold());
            println!("    ├─ Mode:             {:?}", control.mode_run);
            println!("    ├─ KV Cache Quant:   {:?}", control.kv_cache_quantization);
            println!("    ├─ Context Shifting: {:?}", control.context_shifting);
            println!("    ├─ Spec. Decoding:   {:?}", control.speculative_decoding);
            println!("    ├─ Turbo Quant:      {:?}", control.turbo_quant);
            println!("    ├─ Flash Attention:  {:?}", control.flash_attention);
            println!("    ├─ Auto Round:       {:?}", control.auto_round);
            println!("    ├─ VRAM Reclaim:     {:?}", control.force_vram_reclaim);
            println!("    ├─ Memory Lock:      {:?}", control.force_memory_lock);
            println!("    ├─ DFlash:           {:?}", control.dflash);
            println!("    ├─ Think Mode:       {:?}", control.think_mode);
            println!("    ├─ Response Length:  {}", control.response_length);
            println!("    └─ N GPU Layers:     {}", control.n_gpu_layers);

            let options = vec![
                "Mode (Execution Profile)",
                "KV Cache Quantization",
                "Context Shifting",
                "Speculative Decoding",
                "Turbo Quantization",
                "Flash Attention",
                "Auto Round",
                "DFlash",
                "Force VRAM Reclaim",
                "Force Memory Lock",
                "Think Mode",
                "Response Length",
                "N GPU Layers",
                "💾 Save & Exit",
                "❌ Cancel"
            ];

            let choice = inquire::Select::new("\nSelect setting to modify:", options).with_help_message("").prompt()?;

            match choice {
                "Mode (Execution Profile)" => {
                    let modes = vec!["balance", "multitasking", "edge", "max_boost", "ultra_max_boost", "hyper_cluster"];
                    if let Ok(m) = inquire::Select::new("Mode:", modes).with_help_message("").prompt() {
                        control.mode_run = match m {
                            "edge" => BoosterMode::Edge,
                            "multitasking" => BoosterMode::Multitasking,
                            "balance" => BoosterMode::Balance,
                            "max_boost" => BoosterMode::MaxBoost,
                            "ultra_max_boost" => BoosterMode::UltraMaxBoost,
                            "hyper_cluster" => BoosterMode::HyperCluster,
                            _ => control.mode_run,
                        };
                        let _ = HardwareGovernor::save_booster_settings(&control);
                    }
                }
                "KV Cache Quantization" => {
                    let kv_opts = vec!["Auto", "Kv16", "Kv8", "Kv4"];
                    if let Ok(kv) = inquire::Select::new("KV Quantization:", kv_opts).with_help_message("").prompt() {
                        control.kv_cache_quantization = match kv {
                            "Auto" => KvCacheQuantization::Auto,
                            "Kv16" => KvCacheQuantization::Kv16,
                            "Kv8" => KvCacheQuantization::Kv8,
                            "Kv4" => KvCacheQuantization::Kv4,
                            _ => control.kv_cache_quantization,
                        };
                        let _ = HardwareGovernor::save_booster_settings(&control);
                    }
                }
                "Context Shifting" => {
                    let cs_opts = vec!["Auto", "Off", "Minimal", "Standard", "Aggressive", "Extreme"];
                    if let Ok(cs) = inquire::Select::new("Context Shifting:", cs_opts).with_help_message("").prompt() {
                        control.context_shifting = match cs {
                            "Auto" => ContextShiftingMode::Auto,
                            "Off" => ContextShiftingMode::Off,
                            "Minimal" => ContextShiftingMode::Minimal,
                            "Standard" => ContextShiftingMode::Standard,
                            "Aggressive" => ContextShiftingMode::Aggressive,
                            "Extreme" => ContextShiftingMode::Extreme,
                            _ => control.context_shifting,
                        };
                        let _ = HardwareGovernor::save_booster_settings(&control);
                    }
                }
                "DFlash" => {
                    let dflash_opts = vec!["Auto", "On", "Off"];
                    if let Ok(d) = inquire::Select::new("DFlash (FlashKDA):", dflash_opts).with_help_message("").prompt() {
                        control.dflash = cluaiz_shared::hardware::schema::booster::SmartState::Static(d.to_string());
                        let _ = HardwareGovernor::save_booster_settings(&control);
                    }
                }
                "Response Length" => {
                    let rl_opts = vec!["Long", "Short", "Auto"];
                    if let Ok(rl) = inquire::Select::new("Response Length:", rl_opts).with_help_message("").prompt() {
                        control.response_length = rl.to_lowercase();
                        let _ = HardwareGovernor::save_booster_settings(&control);
                    }
                }
                "N GPU Layers" => {
                    let gpu_opts = vec!["GPU Only (Max Acceleration)", "CPU Only (No GPU)", "Hybrid (Custom Layers)"];
                    if let Ok(g_ans) = inquire::Select::new("Compute Architecture:", gpu_opts).with_help_message("").prompt() {
                        match g_ans {
                            "GPU Only (Max Acceleration)" => {
                                control.n_gpu_layers = -1;
                                let _ = HardwareGovernor::save_booster_settings(&control);
                            }
                            "CPU Only (No GPU)" => {
                                control.n_gpu_layers = 0;
                                let _ = HardwareGovernor::save_booster_settings(&control);
                            }
                            "Hybrid (Custom Layers)" => {
                                if let Ok(val) = inquire::Text::new("Enter N GPU Layers (e.g. 10):").with_help_message("").prompt() {
                                    if let Ok(num) = val.parse::<i32>() {
                                        control.n_gpu_layers = num;
                                        let _ = HardwareGovernor::save_booster_settings(&control);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "💾 Save & Exit" => break,
                "❌ Cancel" => return Ok(()),
                other => {
                    // All FeatureState toggles
                    let fs_opts = vec!["Auto", "On", "Off"];
                    if let Ok(fs) = inquire::Select::new(&format!("Set {}:", other), fs_opts).with_help_message("").prompt() {
                        let state = match fs {
                            "On" => FeatureState::On,
                            "Off" => FeatureState::Off,
                            _ => FeatureState::Auto,
                        };
                        match other {
                            "Speculative Decoding" => control.speculative_decoding = state,
                            "Turbo Quantization" => control.turbo_quant = state,
                            "Flash Attention" => control.flash_attention = state,
                            "Auto Round" => control.auto_round = state,
                            "Force VRAM Reclaim" => control.force_vram_reclaim = state,
                            "Force Memory Lock" => control.force_memory_lock = state,
                            "Think Mode" => control.think_mode = state,
                            _ => {}
                        }
                        let _ = HardwareGovernor::save_booster_settings(&control);
                    }
                }
            }
        }
    }

    Ok(())
}
