//! Sovereign Implementation B: Acceleration Pipeline (With Binary Fallback).

use cluaiz_shared::backend::context::cluaizContext;
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tracing::{info, error};

pub struct RuntimeBPipeline;

impl RuntimeBPipeline {
    pub async fn execute_stream(
        model_path: &str,
        context: &cluaizContext,
        prompt: &str,
        _max_tokens: usize,
        mut callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> anyhow::Result<()> {
        info!("🚀 [Llama] Engaging Bare-Metal Binary Driver for: {}", model_path);

        // ── OS & Hardware Aware Routing ──
        let binary_path = crate::router::BinaryRouter::resolve_binary();
        if !binary_path.exists() {
             error!("❌ Binary Sanctum Empty! Archer cannot locate: {:?}", binary_path);
             return Err(anyhow::anyhow!("Missing binary at {:?}", binary_path));
        }

        // 🧬 Extract Model Requirement from DNA
        let requires_gpu = context.dna.dynamic_attributes
            .get("requires_gpu")
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        // Apply template via templater
        let wrapped_prompt = context.templater.format(&context.dna, prompt);

        // Resolve model path to absolute
        let model_path_buf = PathBuf::from(model_path);
        let model_path_str = if model_path_buf.is_absolute() {
            model_path_buf.to_string_lossy().into_owned()
        } else {
             let mut p = std::env::current_dir().unwrap_or_default();
             if p.ends_with("cli") { p.pop(); }
             p.join(model_path).to_string_lossy().into_owned()
        };

        info!("🔥 [Binary Driver] Model: {}", model_path_str);

        // 🧬 Build Dynamic Arguments via Router
        let mut base_args = vec![
            "-m".to_string(), model_path_str,
            "-p".to_string(), wrapped_prompt,
            "-n".to_string(), "256".to_string(),
            "--temp".to_string(), "0.7".to_string(),
            "--ctx-size".to_string(), "2048".to_string(),
            "--no-display-prompt".to_string(),
            "--mlock".to_string(), // Keep mlock hardcoded for now, or dynamic later
        ];

        // 🧠 Dynamically inject missing GGUF arguments
        let metadata = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
        
        // 1. MMAP Toggle
        if metadata.hardware_and_execution.no_mmap {
            base_args.push("--no-mmap".to_string());
        } else {
            base_args.push("--mmap".to_string());
        }

        // 2. Batch Sizes
        base_args.push("-b".to_string());
        base_args.push(metadata.hardware_and_execution.batch_size.to_string());
        base_args.push("-ub".to_string());
        base_args.push(metadata.hardware_and_execution.ubatch_size.to_string());

        // 3. Parallel Sequences
        if metadata.hardware_and_execution.parallel > 1 {
            base_args.push("-np".to_string());
            base_args.push(metadata.hardware_and_execution.parallel.to_string());
        }

        // 4. Override Tensor (KV)
        let override_tensor = metadata.hardware_and_execution.override_tensor.trim();
        if !override_tensor.is_empty() {
            base_args.push("--override-kv".to_string());
            base_args.push(override_tensor.to_string());
        }

        let compute_args = crate::router::BinaryRouter::get_compute_args(requires_gpu);
        base_args.extend(compute_args);

        // 🧠 Dynamically inject Templating Flags
        let chat_template = metadata.templating_flags.chat_template_file.trim();
        if !chat_template.is_empty() {
            base_args.push("--chat-template-file".to_string());
            base_args.push(chat_template.to_string());
            info!("🔥 [Binary Driver] Using Custom Chat Template File: {}", chat_template);
        } else {
            info!("🔥 [Binary Driver] Chat Template: Auto (Reading from GGUF Header)");
        }

        let kwargs = metadata.templating_flags.chat_template_kwargs.trim();
        if !kwargs.is_empty() {
            // Note: Currently most llama.cpp binaries do not natively support --chat-template-kwargs
            // But if the engine/binary fork supports it or processes it downstream:
            info!("🔥 [Binary Driver] Chat Template Kwargs: {}", kwargs);
            // Example flag (assuming the binary fork supports it):
            // base_args.push("--chat-template-kwargs".to_string());
            // base_args.push(kwargs.to_string());
        } else {
            info!("🔥 [Binary Driver] Chat Template Kwargs: Auto");
        }

        if !metadata.templating_flags.jinja {
            // If Jinja is explicitly disabled
            info!("🔥 [Binary Driver] Jinja Formatting: Disabled");
            // base_args.push("--no-jinja".to_string()); // Hypothetical flag
        } else {
            info!("🔥 [Binary Driver] Jinja Formatting: Enabled (Auto)");
        }

        let fit = metadata.templating_flags.fit.trim();
        if !fit.is_empty() && fit != "off" {
            // Google explanation: "Context Window Overflow Controller. Truncates or manages tokens."
            info!("🔥 [Binary Driver] Context Fit Manager: {}", fit);
            if fit == "truncate" {
                // To force truncation of overflowing tokens
                // base_args.push("--keep".to_string());
                // base_args.push("0".to_string()); // Example behaviour
            }
        } else {
            info!("🔥 [Binary Driver] Context Fit Manager: Off (Engine Default)");
        }

        // 🧠 Dynamically inject Speculative Decoding Flags
        let booster = cluaiz_shared::hardware::schema::booster::BoosterControl::load();
        if booster.speculative_decoding.is_active() {
                    let spec_type = metadata.hardware_and_execution.spec_type.as_str();
                    let draft_max = metadata.hardware_and_execution.spec_draft_n_max.to_string();
                    
                    match spec_type {
                        "draft-mtp" => {
                            let draft_path = match &booster.dflash {
                                cluaiz_shared::hardware::schema::booster::SmartState::Custom(cfg) => cfg.draft_model_path.clone(),
                                _ => None,
                            };
                            if let Some(path) = draft_path {
                                base_args.push("-md".to_string());
                                base_args.push(path);
                                base_args.push("--draft".to_string());
                                base_args.push(draft_max.clone());
                                info!("🔥 [Binary Driver] Injected Speculative Decoding (draft-mtp) with max: {}", draft_max);
                            } else {
                                tracing::warn!("⚠️ [Binary Driver] Speculative Decoding is 'draft-mtp' but no draft model path found in Booster dflash config!");
                            }
                        }
                        "ngram-mod" => {
                            base_args.push("--lookup-cache-static".to_string());
                            // Some versions use --lookup-cache or similar. 
                            base_args.push("--draft".to_string());
                            base_args.push(draft_max.clone());
                            info!("🔥 [Binary Driver] Injected Speculative Decoding (ngram-mod) with max: {}", draft_max);
                        }
                        _ => {}
                    }
                }

        let mut child = Command::new(&binary_path)
            .args(&base_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                error!("❌ Failed to spawn llama-cli: {}", e);
                anyhow::anyhow!("Process launch fail: {}", e)
            })?;

        info!("✅ [Binary Driver] Process spawned successfully, reading tokens...");

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let stdout_thread = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buf = [0u8; 256];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                            let _ = tx.send(s.to_string());
                        }
                    },
                    Err(_) => break,
                }
            }
        });
        
        // 🚀 FIX: Drain stderr asynchronously to prevent pipe deadlocks
        let stderr_thread = std::thread::spawn(move || {
            let err_reader = BufReader::new(stderr);
            for line in std::io::BufRead::lines(err_reader).flatten() {
                let lower_line = line.to_lowercase();
                if lower_line.contains("error") || lower_line.contains("assert") {
                     error!("⚠️ [Binary Driver ERROR]: {}", line);
                }
            }
        });
        
        while let Ok(token) = rx.recv() {
            if !token.is_empty() {
                let should_continue = callback(token);
                if !should_continue {
                    break;
                }
            }
        }
        
        stdout_thread.join().ok();
        stderr_thread.join().ok();

        let _ = child.wait();
        info!("🏁 [Binary Driver] Process completed.");
        Ok(())
    }

    pub fn execute_stream_internal(
        _model_path: &str,
        _context: &cluaizContext,
        _prompt: &str,
        _max_tokens: usize,
        _callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("FFI Driver deprecated. Use Binary Driver."))
    }
}
