use color_eyre::Result;
use colored::Colorize;
use cluaiz_shared::hardware::governor::HardwareGovernor;
use cluaiz_shared::hardware::schema::optimization::{
    KvCacheQuantization, ContextShiftingMode, FeatureState
};

pub async fn execute(
    kv_quant: Option<String>,
    context_shift: Option<String>,
    mode: Option<String>,
    spec_decode: Option<String>,
) -> Result<()> {
    let mut control = HardwareGovernor::load_optimization_settings().unwrap_or_default();
    let mut modified = false;

    // Check if any arguments were provided
    let has_args = kv_quant.is_some() || context_shift.is_some() || mode.is_some() || spec_decode.is_some();

    if has_args {
        if let Some(m) = mode {
            let m_clean = m.trim().trim_end_matches("GB").trim_end_matches("gb").trim();
            if m_clean.eq_ignore_ascii_case("auto") {
                control.custom_vram_buffer_gb = None;
                println!("  {} VRAM Safety Buffer set to Auto (% Mode).", "✅".green());
            } else if let Ok(gb) = m_clean.parse::<f64>() {
                control.custom_vram_buffer_gb = Some(gb);
                println!("  {} VRAM Safety Buffer set to {:.2} GB.", "✅".green(), gb);
            } else {
                println!("⚠️  Invalid VRAM safety buffer '{}'. Expected 'auto' or a numeric GB value (e.g. 1.5).", m);
            }
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
        println!("\n  {} {}", "🚀".cyan(), "cluaiz LLM Optimization - Interactive Performance Setup".bold());
        
        loop {
            let gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
            println!("\n  {} {}", "📊".cyan(), "Current LLM Optimization Settings:".bold());
            println!("    ├─ VRAM Safety Buffer: {}", match control.custom_vram_buffer_gb {
                Some(gb) => format!("{:.2} GB (Direct GB)", gb),
                None => "Auto (% Mode)".to_string(),
            });
            println!("    ├─ KV Cache Quant:   {:?}", control.kv_cache_quantization);
            println!("    ├─ Context Shifting: {:?}", control.context_shifting);
            println!("    ├─ Spec. Decoding:   {:?}", control.speculative_decoding);
            println!("    ├─ Turbo Quant:      {:?}", control.turbo_quant);
            println!("    ├─ Flash Attention:  {:?}", control.flash_attention);
            println!("    ├─ Auto Round:       {:?}", control.auto_round);
            println!("    ├─ VRAM Reclaim:     {:?}", control.force_vram_reclaim);
            println!("    ├─ Memory Lock:      {:?}", control.force_memory_lock);
            println!("    ├─ DFlash:           {:?}", control.dflash);
            println!("    ├─ Think Mode:       {:?}", gguf_meta.user_moved_flags.think_mode);
            println!("    ├─ Response Length:  {} defined", gguf_meta.user_moved_flags.response_length.len());
            println!("    └─ N GPU Layers:     {}", gguf_meta.hardware_and_execution.n_gpu_layers);

            let options = vec![
                "VRAM Safety Buffer (Auto / Custom GB)",
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
                "VRAM Safety Buffer (Auto / Custom GB)" => {
                    let buf_opts = vec!["Auto (% Mode)", "Custom Direct GB (e.g. 1.5 GB)"];
                    if let Ok(b) = inquire::Select::new("Select VRAM Buffer Mode:", buf_opts).with_help_message("").prompt() {
                        if b == "Auto (% Mode)" {
                            control.custom_vram_buffer_gb = None;
                            println!("  {} VRAM Safety Buffer set to Auto (% Mode).", "✅".green());
                        } else {
                            if let Ok(val_str) = inquire::Text::new("Enter VRAM Safety Buffer in GB (e.g. 1.5):").with_help_message("").prompt() {
                                let val_clean = val_str.trim().trim_end_matches("GB").trim_end_matches("gb").trim();
                                if let Ok(gb) = val_clean.parse::<f64>() {
                                    control.custom_vram_buffer_gb = Some(gb);
                                    println!("  {} VRAM Safety Buffer set to {:.2} GB.", "✅".green(), gb);
                                } else {
                                    println!("  ⚠️ Invalid GB value '{}'. Keeping current value.", val_str);
                                }
                            }
                        }
                        let _ = HardwareGovernor::save_optimization_settings(&control);
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
                        let _ = HardwareGovernor::save_optimization_settings(&control);
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
                        let _ = HardwareGovernor::save_optimization_settings(&control);
                    }
                }
                "DFlash" => {
                    let dflash_opts = vec!["Auto", "On", "Off"];
                    if let Ok(d) = inquire::Select::new("DFlash:", dflash_opts).with_help_message("").prompt() {
                        control.dflash = cluaiz_shared::hardware::schema::optimization::SmartState::Static(d.to_string());
                        let _ = HardwareGovernor::save_optimization_settings(&control);
                    }
                }
                "Response Length" => {
                    let rl_opts = vec!["Long", "Short", "Auto"];
                    if let Ok(rl) = inquire::Select::new("Response Length:", rl_opts).with_help_message("").prompt() {
                        let mut gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
                        gguf_meta.user_moved_flags.response_length.insert("default".to_string(), rl.to_lowercase());
                        let _ = gguf_meta.save();
                    }
                }
                "N GPU Layers" => {
                    let gpu_opts = vec!["GPU Only (Max Acceleration)", "CPU Only (No GPU)", "Hybrid (Custom Layers)"];
                    if let Ok(g_ans) = inquire::Select::new("Compute Architecture:", gpu_opts).with_help_message("").prompt() {
                        let mut gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
                        match g_ans {
                            "GPU Only (Max Acceleration)" => {
                                gguf_meta.hardware_and_execution.n_gpu_layers = -1;
                                let _ = gguf_meta.save();
                            }
                            "CPU Only (No GPU)" => {
                                gguf_meta.hardware_and_execution.n_gpu_layers = 0;
                                let _ = gguf_meta.save();
                            }
                            "Hybrid (Custom Layers)" => {
                                if let Ok(val) = inquire::Text::new("Enter N GPU Layers (e.g. 10):").with_help_message("").prompt() {
                                    if let Ok(num) = val.parse::<i32>() {
                                        gguf_meta.hardware_and_execution.n_gpu_layers = num;
                                        let _ = gguf_meta.save();
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
                            "Think Mode" => {
                                let mut gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
                                gguf_meta.user_moved_flags.think_mode = match state {
                                    FeatureState::On => "On".to_string(),
                                    FeatureState::Off => "Off".to_string(),
                                    _ => "Auto".to_string(),
                                };
                                let _ = gguf_meta.save();
                            },
                            _ => {}
                        }
                        let _ = HardwareGovernor::save_optimization_settings(&control);
                    }
                }
            }
        }
    }

    Ok(())
}
