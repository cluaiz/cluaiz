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
            "--mmap".to_string(),
            "--mlock".to_string(),
        ];

        let compute_args = crate::router::BinaryRouter::get_compute_args(requires_gpu);
        base_args.extend(compute_args);

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
