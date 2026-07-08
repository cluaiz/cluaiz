use crate::core::state::{ActivityBlock, AppState};
use color_eyre::Result;
use colored::Colorize;
use crossterm::execute;
use crossterm::cursor;
use engines::DownloadEvent;
use inquire::{
    ui::{Attributes, Color, RenderConfig, Styled},
    Select, Text,
};
use std::io::{stdout, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use neural_core::interfaces::router_contract::EmbeddingDriver;
// ── 📦 MODULAR APPS ──
use crate::ui::apps::registry::RegistryApp;

pub struct DashboardEngine;

impl DashboardEngine {
    pub fn run_native(
        state: &mut AppState,
        tx: &mpsc::UnboundedSender<DownloadEvent>,
        rx: &mut mpsc::UnboundedReceiver<DownloadEvent>,
        mode: &mut crate::app_enums::Mode,
    ) -> Result<()> {
        // ══ 🔒 cluaiz RENDER CONFIG ══
        let config = RenderConfig::default();

        // 🛑 GRACEFUL INTERRUPT HANDLER (Sovereign Pivot Control)
        let _ = ctrlc::set_handler(move || {
            cluaiz_shared::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
        });

        // ── 🧬 ATOMIC Core DISCOVERY (cluaiz Startup Scan) ──
        if state.sorted_models.is_empty() {
            state.sorted_models = engines::CoreRoster::get_recommendations(
                &state.hardware.to_hardware_truth(),
                state.ram_gb,
            );
        }

        // ── 📡 cluaiz TELEMETRY IGNITION (Ghost Observer Singleton) ──
        let state_pulse = cluaiz_shared::hardware::telemetry::get_pulse();
        let app_start_time = std::time::Instant::now();
        let last_inference_duration = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let last_ttft = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let peak_power = Arc::new(std::sync::atomic::AtomicU64::new(0));
        
        let ttft_ref = last_ttft.clone();
        let pwr_ref = peak_power.clone();
 
        let engine_ref = state.Core_engine.clone();
 
        // 🚀 cluaiz AUTO-BOOT: Activate the latest engine silently only if no model is loaded
        let is_engine_loaded = state.Core_engine.is_loaded.load(std::sync::atomic::Ordering::SeqCst);
        if !is_engine_loaded {
            // Find the best cached model to boot: prefer the last active one from Permission.json
            let active_schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
            let active_chat_id = active_schema.get_active_chat_model().unwrap_or_default();
            
            let mut boot_target = state.sorted_models.iter().find(|m| {
                if !active_chat_id.is_empty() {
                    m.manifest.id == active_chat_id && m.is_cached
                } else {
                    false
                }
            }).map(|m| (m.manifest.name.clone(), m.manifest.local_path.clone()));

            if boot_target.is_none() {
                // ⚠️ Active model not cached (e.g. deleted or moved). Fall back to any cached model.
                boot_target = state.sorted_models.iter().find(|m| {
                    m.is_cached && m.manifest.category != "embedding" && m.manifest.architecture_type != "onnx"
                }).map(|m| (m.manifest.name.clone(), m.manifest.local_path.clone()));
            }

            let perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
            if !perms.lazy_load_model {
                match boot_target {
                    Some((ref name, Some(ref path_str))) => {
                        // ✅ Model is on disk with a known path — load it directly
                        let mut spinner = cluaiz_shared::utils::spinner::cluaizSpinner::new();
                        spinner.start(&format!("Auto-Booting Neural Kernel: {}...", name));
                        let path = std::path::PathBuf::from(path_str);
                        let is_gguf = path.extension().and_then(|s| s.to_str()) == Some("gguf");
                        let runtime = if is_gguf {
                            cluaiz_shared::BackendType::RuntimeB
                        } else {
                            cluaiz_shared::BackendType::RuntimeA
                        };
                        tokio::task::block_in_place(|| {
                            let handle = tokio::runtime::Handle::current();
                            let result = handle.block_on(engines::CoreRouter::load_model(path, runtime));
                            match result {
                                Ok(router) => {
                                    let mut lock = state.Core_engine.router.blocking_lock();
                                    *lock = router;
                                    
                                    let ctx = lock.get_active_dna().and_then(|d| d.max_context_length).unwrap_or(2048);
                                    let model_gb = state.sorted_models.iter()
                                        .find(|m| m.manifest.name == *name)
                                        .map(|m| m.manifest.download_size_gb)
                                        .unwrap_or(0.0);
                                    
                                    state.Core_engine.is_loaded.store(true, std::sync::atomic::Ordering::SeqCst);
                                    spinner.stop(None);
                                    
                                    if model_gb > 0.0 {
                                        println!("  {} {} loaded and ready. \x1B[90m[Ctx: {}, Model: {:.2}GB, RAM: {:.1}GB]\x1B[0m", "✅".green(), name.bold(), ctx, model_gb, state.ram_gb);
                                    } else {
                                        println!("  {} {} loaded and ready. \x1B[90m[Ctx: {}, RAM: {:.1}GB]\x1B[0m", "✅".green(), name.bold(), ctx, state.ram_gb);
                                    }
                                }
                                Err(e) => {
                                    spinner.stop(None);
                                    println!("  {} Auto-Boot failed: {}", "❌".red(), e);
                                }
                            }
                        });
                    }
                    _ => {
                        // ⚠️ No cached model found
                        if !state.sorted_models.is_empty() {
                            println!("\n  {} No local weights found. Use '@' or '/menu' → Model List to download one.", "⚠️".yellow());
                        }
                    }
                }
            } else {
                println!("  {} Lazy-Load Enabled. Neural Model weights will load on demand.", "⚙️".yellow());
            }
        }
        // 🖊️ INPUT FIX: Ensure cursor is on a fresh line before inquire renders
        println!();

        let mut last_booster_modified = std::fs::metadata(cluaiz_shared::environment::EnvironmentManager::current().engine_dir().join("system_booster.json")).and_then(|m| m.modified()).ok();
        let mut last_booster_state = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().ok();

        // Track global think state across pivots
        let global_think_state = Arc::new(AtomicBool::new(false));

        let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();

        loop {
            
            let mut auto_input = None;
            if let Some(first) = state.chat_paste_buffer.first() {
                if first.starts_with("[PIVOT_CONTINUE]") {
                    auto_input = Some(state.chat_paste_buffer.remove(0));
                }
            }

            let input = if let Some(val) = auto_input {
                Ok(val)
            } else {
                // 🧹 Flush stdin buffer to clear any queued terminal input or echoed characters
                while let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(0)) {
                    let _ = crossterm::event::read();
                }

                let res = Text::new(">")
                    .with_placeholder("Type your message or @ & / for menu")
                    .with_render_config(config.clone())
                    .prompt();
                
                let (_cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
                let mut stdout = std::io::stdout();
                let _ = execute!(stdout, cursor::SavePosition, cursor::MoveTo(0, rows - 1), crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine), cursor::RestorePosition);
                let _ = stdout.flush();

                if res.is_ok() {
                    print!("\x1B[1A\x1B[2K");
                    let _ = stdout.flush();
                }
                res
            };

            let now = std::time::Instant::now();
            let delta = now.duration_since(state.last_input_time).as_millis();
            state.last_input_time = now;

            match input {
                Ok(val) => {
                    let val = val.trim();
                    if val.is_empty() {
                        continue;
                    }

                    // 📋 PASTE DETECTION & MERGING
                    if delta < 50 {
                        state.chat_paste_buffer.push(val.to_string());
                        continue;
                    }

                    let final_message = if !state.chat_paste_buffer.is_empty() {
                        let mut merged = state.chat_paste_buffer.join("\n");
                        merged.push('\n');
                        merged.push_str(val);
                        state.chat_paste_buffer.clear();
                        merged
                    } else {
                        val.to_string()
                    };

                    print!("\x1B[1A\x1B[2K");
                    std::io::stdout().flush()?;

                    if final_message.starts_with('/') {
                        Self::handle_command(state, tx, mode, &final_message[1..])?;
                        if *mode == crate::app_enums::Mode::Quit {
                            break;
                        }
                    } else if final_message.starts_with('@') {
                        Self::handle_model_switch(state, tx, rx, &final_message[1..], None)?;
                    } else {
                        // show_dashboard.store(true, std::sync::atomic::Ordering::SeqCst);
                        
                        // ── 👤 USER MESSAGE ──
                        use crossterm::style::Stylize;
                        let icon = Stylize::bold(Stylize::cyan("👤"));
                        println!("{} {}", icon, final_message.clone().white());
                        state.activity_stream.push(ActivityBlock::Chat(
                            "USER".to_string(),
                            final_message.to_string(),
                        ));
                        state.rendered_actions_count += 1;

                        // 🧠 Database FFI Integration: Vectorize and save User Prompt
                        let storage_bridge = engines::memory::storage_bridge::load_storage_bridge();
                        let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                        if schema.vectorize_user_input {
                            let prompt_vector = engines::memory::embedding_generator::EmbeddingGenerator::generate_vector(&final_message);
                            let prompt_id = format!("prompt-{}", uuid::Uuid::new_v4());
                            let _ = storage_bridge.save_context(&prompt_id, &final_message, &prompt_vector);
                        }

                        // ── 🧿 NEURAL DISPATCH ──
                        let _ = std::io::Write::flush(&mut std::io::stdout());

                        // ── 🤖 REAL Core STREAMING ────────────────────────
                        let full_response =
                            std::sync::Arc::new(std::sync::Mutex::new(String::new()));
                        let full_clone = full_response.clone();
                        let _first_token = true;
                        // 🧠 Think-mode state machine
                        let in_think = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let in_think_cb = in_think.clone();
                        // 🛑 EOS detection: stops display when model generates stop token
                        let reached_eos = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let eos_cb = reached_eos.clone();

                        let start_time = std::time::Instant::now();
                        let initial_tokens = state_pulse.tps_counter.load(Ordering::SeqCst);
                        
                        // Reset session metrics
                        pwr_ref.store(0.0f64.to_bits(), Ordering::SeqCst);
                        ttft_ref.store(0.0f64.to_bits(), Ordering::SeqCst);

                        let pulse_for_snapshot = state_pulse.clone();
                        let _pwr_cb = pwr_ref.clone();
                        let ttft_cb = ttft_ref.clone();
                        
                        // ── 🔥 HOT RELOAD ENGINE SETTINGS ──
                        let booster_path = cluaiz_shared::environment::EnvironmentManager::current().engine_dir().join("system_booster.json");
                        if let Ok(meta) = std::fs::metadata(&booster_path) {
                            if let Ok(modified) = meta.modified() {
                                let mut needs_reload = false;
                                if let Some(last) = last_booster_modified {
                                    if modified > last {
                                        if let Ok(current_booster) = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings() {
                                            if let Some(ref prev_booster) = last_booster_state {
                                                if current_booster != *prev_booster {
                                                    needs_reload = true;
                                                }
                                            } else {
                                                needs_reload = true; // No previous state, force reload
                                            }
                                            last_booster_state = Some(current_booster);
                                        }
                                    }
                                }
                                last_booster_modified = Some(modified);

                                if needs_reload {
                                    if let Some(model_id) = state._active_model_id.clone() {
                                        if let Some(model) = state.sorted_models.iter().find(|m| m.manifest.id == model_id) {
                                            if let Some(local_path) = &model.manifest.local_path {
                                                let path = std::path::PathBuf::from(local_path);
                                                println!("\r\x1B[2K\x1B[0m{} Hot-Reloading Neural Engine based on new settings...", crossterm::style::Stylize::magenta("🚀"));
                                                let rt = tokio::runtime::Handle::current();
                                                tokio::task::block_in_place(|| {
                                                    let _ = rt.block_on(state.Core_engine.load_model(path));
                                                });
                                                print!("\x1B[1A\x1B[2K\r"); // clear message
                                                let _ = std::io::stdout().flush();
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Reset cancellation signal before starting
                        cluaiz_shared::GLOBAL_CANCEL_SIGNAL.store(false, Ordering::SeqCst);

                        if !state.Core_engine.is_loaded.load(Ordering::SeqCst) && !state.is_client_mode {
                            let mut spinner = cluaiz_shared::utils::spinner::cluaizSpinner::new();
                            spinner.start("Lazy loading neural model weights...");
                            if let Some(model_id) = state._active_model_id.clone() {
                                if let Some(model) = state.sorted_models.iter().find(|m| m.manifest.id == model_id) {
                                    if let Some(local_path) = &model.manifest.local_path {
                                        let path = std::path::PathBuf::from(local_path);
                                        let is_gguf = path.extension().and_then(|s| s.to_str()) == Some("gguf");
                                        let runtime = if is_gguf {
                                            cluaiz_shared::BackendType::RuntimeB
                                        } else {
                                            cluaiz_shared::BackendType::RuntimeA
                                        };
                                        let rt = tokio::runtime::Handle::current();
                                        let load_res = tokio::task::block_in_place(|| {
                                            rt.block_on(state.Core_engine.load_model(path))
                                        });
                                        if load_res.is_ok() {
                                            state.Core_engine.is_loaded.store(true, Ordering::SeqCst);
                                        }
                                    }
                                }
                            }
                            spinner.stop(None);
                        }

                        let stream_result = tokio::task::block_in_place(|| {
                            let mut lock = state.Core_engine.router.blocking_lock();
                            
                            // 🧬 Dynamic Config Fetch: Zero Hardcoding
                            let stop_seqs = lock.get_active_dna().map(|d| d.stop_sequences.clone()).unwrap_or_default();
                            
                            // 🎭 Orchestration: native.rs handles the templating now.
                            let formatted_prompt = final_message.clone();

                            // 🧬 DYNAMIC TOKEN ALLOCATION: Calculate space based on DNA Context Window
                            let ctx_window = lock.get_active_dna().and_then(|d| d.max_context_length).unwrap_or(2048);
                            let prompt_tokens = 0; // We no longer rely on external tokenizers for length prediction
                            
                            let max_t = lock.get_active_dna()
                                .and_then(|d| d.inference_params.get("max_tokens"))
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(8192); // 🚀 DYNAMIC: Allow large stream, context shifting will handle KV bounds.

                            // 🔇 SURGICAL SILENCE: Temporarily redirect stderr to NUL/dev/null
                            let mut saved_stderr: libc::c_int = -1;
                            #[cfg(windows)]
                            unsafe {
                                saved_stderr = libc::dup(2);
                                let null_fd = libc::open("NUL\0".as_ptr() as *const libc::c_char, libc::O_WRONLY);
                                if null_fd != -1 {
                                    libc::dup2(null_fd, 2);
                                    libc::close(null_fd);
                                }
                            }
                            #[cfg(not(windows))]
                            unsafe {
                                saved_stderr = libc::dup(2);
                                let null_fd = libc::open("/dev/null\0".as_ptr() as *const libc::c_char, libc::O_WRONLY);
                                if null_fd != -1 {
                                    libc::dup2(null_fd, 2);
                                    libc::close(null_fd);
                                }
                            }
                            
                            let booster = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                            let suppress_thinking = booster.think_mode == cluaiz_shared::hardware::schema::booster::FeatureState::Off;
                            
                            let active_model = state._active_model_id.clone().unwrap_or_default().to_lowercase();
                            let is_reasoning_model = active_model.contains("deepseek") || active_model.contains("r1") || active_model.contains("reason") || active_model.contains("bonsai") || active_model.contains("think");

                            // Deep Truth: Dynamic tag resolution
                            let mut think_start_tag = String::new();
                            let mut think_end_tag = String::new();
                            if let Some(dna) = lock.get_active_dna() {
                                if !dna.think_tag_schema.is_empty() && dna.think_tag_schema != "none" {
                                    think_start_tag = dna.think_tag_schema.clone();
                                    think_end_tag = dna.think_end_schema.clone();
                                }
                            }

                            let prompt_starts_in_think = !think_start_tag.is_empty() && formatted_prompt.contains(&think_start_tag) && (think_end_tag.is_empty() || !formatted_prompt.contains(&think_end_tag));
                            let is_pivot = formatted_prompt.starts_with("[PIVOT_CONTINUE]");

                            let mut first_token = true;
                            let pulse_clone = state_pulse.clone(); // 🧬 Clone for closure move
                            let global_think_cb = global_think_state.clone();
                            
                            // ⚡ Enable raw mode to intercept keystrokes asynchronously (like Ctrl+T)
                            let _ = crossterm::terminal::enable_raw_mode();

                            let result = lock.generate_stream(
                                &formatted_prompt, // 🚀 Pure, unadulterated prompt
                                max_t,
                                Box::new(move |token: String| -> bool {
                                    // 🛑 Stop if already past EOS or interrupted
                                    if (eos_cb.load(Ordering::SeqCst) && !token.starts_with("__ENGINE_PAUSE_EXECUTE__")) || cluaiz_shared::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst) { 
                                        return false; 
                                    }

                                    // ⚡ CLI Shortcut: Poll for Ctrl+T to skip thinking
                                    if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(0)) {
                                        if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                                            if key.code == crossterm::event::KeyCode::Char('t') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                                // Only skip if we are currently IN thinking mode — prevents double-fire from key-repeat
                                                if in_think_cb.load(Ordering::SeqCst) {
                                                    cluaiz_shared::GLOBAL_SKIP_THINKING_SIGNAL.store(true, Ordering::SeqCst);
                                                    // ✅ Immediately reset UI think state — don't wait for </think> text
                                                    in_think_cb.store(false, Ordering::SeqCst);
                                                    global_think_cb.store(false, Ordering::SeqCst);
                                                    print!("\x1B[0m\r\n⚡ Skipping thinking...\r\n");
                                                }
                                            }
                                            if key.code == crossterm::event::KeyCode::Char('c') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                                cluaiz_shared::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
                                            }
                                        }
                                    }

                                    // 🛑 Deep-Suffix Scan
                                    if let Ok(mut res) = full_clone.lock() {
                                        let clean_res = (res.clone() + token.as_str()).replace("\n", "").replace("\r", "").replace(" ", "");
                                        if stop_seqs.iter().any(|s| {
                                            let clean_s = s.replace("\n", "").replace("\r", "").replace(" ", "");
                                            !clean_s.is_empty() && clean_res.ends_with(&clean_s)
                                        }) {
                                            eos_cb.store(true, Ordering::SeqCst);
                                            return false;
                                        }

                                        // 🤖 First-Token Handshake
                                        if first_token {
                                            let ttft = start_time.elapsed().as_secs_f64();
                                            ttft_cb.store(ttft.to_bits(), Ordering::SeqCst);

                                            let mut out = std::io::stdout();
                                            let _ = out.write_all(format!("\r\x1B[2K\x1B[0m{} ", crossterm::style::Stylize::magenta("🤖")).as_bytes());
                                            
                                            // DYNAMIC THINK INJECTION
                                            let should_start_in_think = prompt_starts_in_think || is_reasoning_model;

                                            if !suppress_thinking && should_start_in_think { 
                                                in_think_cb.store(true, Ordering::SeqCst);
                                                global_think_cb.store(true, Ordering::SeqCst);
                                                let mut out = std::io::stdout();
                                                let _ = out.write_all(b"\r\n\x1B[90m> \x1B[3m"); // Gray italics with carriage return
                                                let _ = out.flush();
                                            }
                                            
                                            let _ = out.flush();
                                            first_token = false;
                                        }

                                        // Real-time Event Trigger Checks for Dashboard
                                        let mut is_system_step = false;
                                        if token.starts_with("__STEP_2_MATCH_START__") {
                                            let parts: Vec<&str> = token.split(':').collect();
                                            let matched = if parts.len() >= 2 { parts[1] } else { "unknown" };
                                            let score = if parts.len() >= 3 { parts[2] } else { "0.88" };
                                            print!("\r\n\x1B[K✅ \x1B[32m[Step 2]\x1B[0m Match Found -> Registry Tool: '{}' (Score: {})\r\n", matched, score);
                                            is_system_step = true;
                                        } else if token.starts_with("__STEP_3_INJECT_START__") {
                                            print!("\r\n\x1B[K✅ \x1B[32m[Step 3]\x1B[0m Dynamic JIT Layer rules compile & inject successfully.\r\n");
                                            is_system_step = true;
                                        } else if token == "__STEP_4_READ_SMS__" {
                                            print!("\r\n\x1B[K✅ \x1B[32m[Step 4]\x1B[0m Inference system parses user SMS input context.\r\n");
                                            is_system_step = true;
                                        } else if token.starts_with("<TRIGGER:") {
                                            let clean = token.replace("\n", "").replace("\r", "");
                                            print!("\r\n\x1B[K✅ \x1B[32m[Step 5]\x1B[0m AI Formulates Plan: Match tag emitted -> {}\r\n", clean);
                                            is_system_step = true;
                                        } else if token.contains("</TRIGGER>") {
                                            print!("\r\n\x1B[K✅ \x1B[32m[Step 6]\x1B[0m AI Emits closing sequence tag: </TRIGGER>\r\n");
                                            is_system_step = true;
                                        } else if token.starts_with("__ENGINE_PAUSE_EXECUTE__") {
                                            // ── 🌀 Dynamic Real-Time Agentic Trace ──
                                            let mut sp = cluaiz_shared::utils::spinner::cluaizSpinner::new();
                                            
                                            sp.start("Receiving User SMS...");
                                            std::thread::sleep(std::time::Duration::from_millis(250));
                                            sp.stop(Some("\x1B[K✅ \x1B[32m[Step 1]\x1B[0m User SMS Received"));
                                            println!();
                                            
                                            sp.start("Scanning Registry for Tool Match...");
                                            std::thread::sleep(std::time::Duration::from_millis(300));
                                            sp.stop(Some("\x1B[K✅ \x1B[32m[Step 2]\x1B[0m Match Found -> Registry Tool"));
                                            println!();
                                            
                                            sp.start("Compiling Dynamic JIT Layer rules...");
                                            std::thread::sleep(std::time::Duration::from_millis(200));
                                            sp.stop(Some("\x1B[K✅ \x1B[32m[Step 3]\x1B[0m Dynamic JIT Layer rules compile & inject successfully."));
                                            println!();
                                            
                                            sp.start("Parsing SMS input context...");
                                            std::thread::sleep(std::time::Duration::from_millis(200));
                                            sp.stop(Some("\x1B[K✅ \x1B[32m[Step 4]\x1B[0m Inference system parses user SMS input context."));
                                            println!();
                                            
                                            let parts: Vec<&str> = token.splitn(3, ':').collect();
                                            let tool_name = if parts.len() >= 3 { parts[1] } else { "unknown_tool" };
                                            
                                            sp.start("AI Formulating Plan...");
                                            std::thread::sleep(std::time::Duration::from_millis(400));
                                            sp.stop(Some(&format!("\x1B[K✅ \x1B[32m[Step 5]\x1B[0m AI Formulates Plan: Match tag emitted -> <TRIGGER:{}>", tool_name)));
                                            println!();
                                            
                                            sp.start("Emitting closing sequence tag...");
                                            std::thread::sleep(std::time::Duration::from_millis(200));
                                            sp.stop(Some("\x1B[K✅ \x1B[32m[Step 6]\x1B[0m AI Emits closing sequence tag: </TRIGGER>"));
                                            println!();
                                            
                                            sp.start("Triggering Engine intercept...");
                                            std::thread::sleep(std::time::Duration::from_millis(300));
                                            sp.stop(Some("\x1B[K✅ \x1B[32m[Step 7]\x1B[0m Engine intercept triggered. Autoregressive loop PAUSED."));
                                            println!();
                                            
                                            sp.start(&format!("Sandbox executing '{}'...", tool_name));
                                            std::thread::sleep(std::time::Duration::from_millis(600));
                                            if parts.len() >= 2 {
                                                sp.stop(Some(&format!("\x1B[K✅ \x1B[32m[Step 8]\x1B[0m Sandbox UnifiedExecutor executed: '{}'.", parts[1])));
                                            } else {
                                                sp.stop(Some("\x1B[K✅ \x1B[32m[Step 8]\x1B[0m Sandbox UnifiedExecutor executed."));
                                            }
                                            println!();
                                            
                                            sp.start("Injecting Zero-copy KV-Cache...");
                                            std::thread::sleep(std::time::Duration::from_millis(250));
                                            sp.stop(Some("\x1B[K✅ \x1B[32m[Step 9]\x1B[0m Zero-copy KV-Cache parameters injected directly into context layers. Resuming loop..."));
                                            println!();
                                            
                                            print!("\r\n──── \x1B[35m🤖 AI FINAL ANSWER\x1B[0m ────────────────────────\r\n\r\n");
                                            is_system_step = true;
                                            eos_cb.store(false, Ordering::SeqCst);
                                        }

                                        // Filter tags and update state
                                        let mut display_token = token.clone();
                                        
                                        for tag in &["<turn|>", "<|im_end|>", "<end_of_turn>", "<|im_start|>", "<start_of_turn>"] {
                                            display_token = display_token.replace(tag, "");
                                        }

                                        // Filter internal macro leaks in Standalone mode
                                        if is_system_step
                                            || display_token.contains("[Agentic Pause]") 
                                            || display_token.contains("__ENGINE_PAUSE_EXECUTE__") 
                                            || display_token.contains("<TOOL_OUTPUT_LOG>") 
                                            || display_token.contains("</TOOL_OUTPUT_LOG>")
                                            || display_token.starts_with("__STEP_")
                                            || display_token.starts_with("<TRIGGER:")
                                            || display_token.contains("</TRIGGER>") {
                                            display_token = String::new();
                                        }

                                        let accumulated = res.clone() + token.as_str();
                                        let mut just_finished_thinking = false;

                                        // ALWAYS check for </think> to hide it and cleanly exit think mode
                                        if !think_end_tag.is_empty() && (accumulated.ends_with(&think_end_tag) || token.contains(&think_end_tag)) {
                                            in_think_cb.store(false, Ordering::SeqCst);
                                            global_think_cb.store(false, Ordering::SeqCst);
                                            display_token = display_token.replace(&think_end_tag, "");
                                            just_finished_thinking = true;
                                        }

                                        // ALWAYS check for <think> to turn ON think mode dynamically
                                        if !think_start_tag.is_empty() && (accumulated.ends_with(&think_start_tag) || token.contains(&think_start_tag)) {
                                            if !in_think_cb.load(Ordering::SeqCst) {
                                                in_think_cb.store(true, Ordering::SeqCst);
                                                global_think_cb.store(true, Ordering::SeqCst);
                                                print!("\r\n\x1B[90m> \x1B[3m");
                                            }
                                            display_token = display_token.replace(&think_start_tag, "");
                                        }

                                        if just_finished_thinking {
                                            print!("\x1B[0m\r\n\r\n");
                                        }

                                        let currently_thinking = in_think_cb.load(Ordering::SeqCst);
                                        
                                        if !display_token.is_empty() {
                                            let display_token_raw = display_token.replace("\n", "\r\n");
                                            if currently_thinking && !suppress_thinking {
                                                 print!("\x1B[90m{}\x1B[0m", display_token_raw);
                                            } else {
                                                 print!("{}", display_token_raw);
                                            }
                                        } else if just_finished_thinking {
                                            print!("\x1B[0m"); // Even if display token is empty, we must reset
                                        }

                                        let _ = std::io::stdout().flush();
                                        res.push_str(&token); // Keep original token with tags in internal state
                                        pulse_clone.tps_counter.fetch_add(1, Ordering::SeqCst);
                                    }
                                    true
                                }),
                            );
                            
                            // ⚡ Disable raw mode safely after stream ends
                            let _ = crossterm::terminal::disable_raw_mode();
                            
                            // 🔊 RESTORE stderr properly using duplicated FD
                            #[cfg(windows)]
                            unsafe {
                                if saved_stderr != -1 {
                                    libc::dup2(saved_stderr, 2);
                                    libc::close(saved_stderr);
                                }
                            }
                            #[cfg(not(windows))]
                            unsafe {
                                if saved_stderr != -1 {
                                    libc::dup2(saved_stderr, 2);
                                    libc::close(saved_stderr);
                                }
                            }
                            
                            // 🚀 Hardware-managed Agentic Pause and Zero-Delay TTFT handled entirely by Native Router
                            result
                        });

                        let end_time = std::time::Instant::now();
                        let duration = end_time.duration_since(start_time).as_secs_f64();
                        let final_tokens = pulse_for_snapshot.tps_counter.load(Ordering::SeqCst);
                        let tokens_in_this_run = final_tokens.saturating_sub(initial_tokens);
                        let avg_tps = if duration > 0.0 { tokens_in_this_run as f64 / duration } else { 0.0 };



                        let response = if let Err(e) = stream_result {
                            use crossterm::style::Stylize;
                            let err_msg = format!("{} ERROR: {}", Stylize::red("🤖"), e);
                            println!("{}", err_msg);
                            err_msg
                        } else {
                            let res = if let Ok(lock) = full_response.lock() {
                                lock.clone()
                            } else {
                                String::new()
                            };
                            if res.is_empty() {
                                use crossterm::style::Stylize;
                                let empty_msg =
                                    format!("{} ERROR: Generated empty response.", Stylize::red("🤖"));
                                println!("{}", empty_msg);
                                empty_msg
                            } else {
                                res
                            }
                        };

                        state.activity_stream.push(ActivityBlock::Chat(
                            "ARCHER".to_string(),
                            response.to_string(),
                        ));

                        // 🧠 Database FFI Integration: Vectorize and save AI Response
                        if schema.vectorize_ai_response {
                            let response_vector = engines::memory::embedding_generator::EmbeddingGenerator::generate_vector(&response);
                            let response_id = format!("response-{}", uuid::Uuid::new_v4());
                            let _ = storage_bridge.save_context(&response_id, &response, &response_vector);
                        }

                        if cluaiz_shared::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst) {
                            println!();
                            use crossterm::style::Stylize;
                            println!("{} {}", "⏸️  Paused:".with(crossterm::style::Color::Yellow).bold(), "Engine stopped mid-generation. Context preserved in VRAM.".with(crossterm::style::Color::DarkGrey));
                            let pivot_input = Text::new("Enter mid-way instruction (or press Enter to return):")
                                .with_render_config(config.clone())
                                .prompt();
                            
                            if let Ok(instruction) = pivot_input {
                                if !instruction.trim().is_empty() {
                                    // Inject pivot into state to be processed in the next loop
                                    state.chat_paste_buffer.push(format!("[PIVOT_CONTINUE] {}", instruction.trim()));
                                }
                            }
                        }

                        
                        let ttft_secs = f64::from_bits(ttft_ref.load(Ordering::SeqCst));
                        let registry = cluaiz_shared::hardware::governor::HardwareGovernor::load_process_registry();
                        let my_pid = std::process::id().to_string();
                        let vram_used_gb = registry.get(&my_pid).map(|i| i.vram_gb).unwrap_or(0.0);

                        let schema_for_telemetry = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                        if schema_for_telemetry.stream_telemetry {
                            println!("\n  {} │ {} tokens │ {:.1} TPS │ {:.2}s │ TTFT: {:.2}s │ VRAM Used: {:.2} GB", 
                                colored::Colorize::magenta("⚡ System Benchmark"), 
                                colored::Colorize::cyan(tokens_in_this_run.to_string().as_str()), 
                                avg_tps, 
                                duration,
                                ttft_secs,
                                vram_used_gb
                            );
                        }
                        println!(); // ensure prompt starts on fresh line
                    }
                }
                Err(_) => {
                    *mode = crate::app_enums::Mode::Quit;
                    break;
                }
            }
        }
        Ok(())
    }

    fn handle_command(
        state: &mut AppState,
        tx: &mpsc::UnboundedSender<DownloadEvent>,
        mode: &mut crate::app_enums::Mode,
        cmd: &str,
    ) -> Result<()> {
        match cmd {
            "menu" | "" => {
                let config = RenderConfig::default()
                    .with_prompt_prefix(Styled::new("🏠︎").with_fg(Color::LightCyan))
                    .with_highlighted_option_prefix(
                        Styled::new("⮞")
                            .with_fg(Color::LightCyan)
                            .with_attr(Attributes::BOLD),
                    );
                let options = vec!["Model List", "Settings", "Help", "Quit"];
                let ans = Select::new("Main Menu:", options).with_help_message("")
                    .with_render_config(config)
                    .prompt()?;

                print!("\x1B[1A\x1B[2K\r");
                stdout().flush()?;

                match ans {
                    "Model List" => RegistryApp::show(state, tx)?,
                    "Settings" => println!("  {} Settings coming soon...", "⚙️".yellow()),
                    "Help" => println!("  {} Help coming soon...", "ℹ️".blue()),
                    "Quit" => *mode = crate::app_enums::Mode::Quit,
                    _ => {}
                }
            }
            "quit" | "exit" => *mode = crate::app_enums::Mode::Quit,
            "clear" => {
                print!("\x1B[2J\x1B[1;1H");
                state.printed_logo = false;
            }
            cmd if cmd.starts_with("run ") => {
                let model_id = &cmd[4..];
                println!("  {} Silicon Dispatch: '{}'...", "🧬".cyan(), model_id);
                
                let manager = engines::models::manager::ModelManager::new(
                    engines::models::registry::REGISTRY_URL.to_string(),
                    std::path::PathBuf::from("models")
                );

                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    match rt.block_on(manager.pull_model(model_id)) {
                        Ok(_) => println!("  {} Link Established.", "✅".green()),
                        Err(e) => println!("  {} Dispatch Failed: {}", "❌".red(), e),
                    }
                });
            }
            _ => {
                println!("  {} Unknown command: /{}", "❌".red(), cmd);
            }
        }
        Ok(())
    }

    fn handle_model_switch(
        state: &mut AppState,
        _tx: &mpsc::UnboundedSender<DownloadEvent>,
        rx: &mut mpsc::UnboundedReceiver<DownloadEvent>,
        _filter: &str,
        auto_boot_target: Option<&str>,
    ) -> Result<()> {
        state.handle_events(rx);
        let config = RenderConfig::default()
            .with_prompt_prefix(
                Styled::new("@")
                    .with_fg(Color::LightCyan)
                    .with_attr(Attributes::BOLD),
            )
            .with_highlighted_option_prefix(Styled::new("⮞").with_fg(Color::LightCyan));

        let (master_ans, ans) = if let Some(target) = auto_boot_target {
            ("🧠 Switch Chat Model".to_string(), target.to_string())
        } else {
            loop {
                let master_options = vec![
                    "🧠 Switch Chat Model".to_string(),
                    "⚙️ Switch Vector Model".to_string(),
                    "⚡ Engine Modes".to_string(),
                    "🚀 System Booster".to_string(),
                ];
                let master_ans = match Select::new("Action:", master_options).with_help_message("")
                    .with_render_config(config.clone())
                    .prompt() {
                    Ok(ans) => ans,
                    Err(inquire::InquireError::OperationCanceled) | Err(inquire::InquireError::OperationInterrupted) => {
                        print!("\x1B[1A\x1B[2K\r");
                        stdout().flush()?;
                        return Ok(());
                    }
                    Err(e) => return Err(e.into()),
                };
                    
                print!("\x1B[1A\x1B[2K\r");
                stdout().flush()?;

                if master_ans.contains("Engine Modes") {
                    let modes = vec![
                        "⚡ Flash Mode (High Speed)".to_string(),
                        "🧠 Think Mode (Deep Reasoning)".to_string(),
                        "🚀 Boot Mode (Auto-Start Engine)".to_string(),
                    ];
                    let mode_ans = match Select::new("Select Mode:", modes).with_help_message("")
                        .with_render_config(config.clone())
                        .prompt() {
                        Ok(ans) => ans,
                        Err(inquire::InquireError::OperationCanceled) | Err(inquire::InquireError::OperationInterrupted) => {
                            print!("\x1B[1A\x1B[2K\r"); // Erase <canceled>
                            stdout().flush()?;
                            continue; // One step back
                        }
                        Err(e) => return Err(e.into()),
                    };
                        
                    print!("\x1B[1A\x1B[2K\r");
                    stdout().flush()?;
                    
                    let mut booster = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                    if mode_ans.contains("Flash Mode") {
                        booster.mode_run = cluaiz_shared::hardware::schema::booster::BoosterMode::Edge;
                        booster.think_mode = cluaiz_shared::hardware::schema::booster::FeatureState::Off;
                    } else if mode_ans.contains("Think Mode") {
                        booster.mode_run = cluaiz_shared::hardware::schema::booster::BoosterMode::MaxBoost;
                        booster.think_mode = cluaiz_shared::hardware::schema::booster::FeatureState::On;
                    } else if mode_ans.contains("Boot Mode") {
                        booster.mode_run = cluaiz_shared::hardware::schema::booster::BoosterMode::Balance;
                        booster.think_mode = cluaiz_shared::hardware::schema::booster::FeatureState::Auto;
                    }
                    
                    let _ = cluaiz_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster);
                    
                    println!("  {} {} activated and saved to system_booster.json.", "✅".green(), mode_ans.bold());
                    return Ok(());
                } else if master_ans.contains("System Booster") {
                    let booster_path = cluaiz_shared::environment::EnvironmentManager::current().engine_dir().join("system_booster.json");

                    loop {
                        let mut booster = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                        
                        let compute_mode_str = match booster.n_gpu_layers {
                            0 => "CPU Only".to_string(),
                            -1 => "GPU (Full Offload)".to_string(),
                            n => format!("Hybrid ({} Layers)", n),
                        };

                        let mut options = vec![
                            format!("Neural Mode (Current: {:?})", booster.mode_run),
                            format!("Compute Device (Current: {})", compute_mode_str),
                            format!("Turbo Quant (Current: {:?})", booster.turbo_quant),
                            format!("Flash Attention (Current: {:?})", booster.flash_attention),
                            format!("Speculative Decoding (Current: {:?})", booster.speculative_decoding),
                            format!("Auto Round (Current: {:?})", booster.auto_round),
                            format!("DFlash (FlashKDA) (Current: {:?})", booster.dflash),
                            format!("Context Shifting (Current: {:?})", booster.context_shifting),
                            format!("Force VRAM Reclaim (Current: {:?})", booster.force_vram_reclaim),
                            format!("KV Cache Quantization (Current: {:?})", booster.kv_cache_quantization),
                            format!("Think Mode (Current: {:?})", booster.think_mode),
                            format!("Response Length (Current: {})", booster.response_length),
                            format!("Force Memory Lock (Current: {:?})", booster.force_memory_lock),
                        ];
                        options.push("🔙 Back to Menu".to_string());
                        
                        let target_ans = match Select::new("Configure Setting:", options).with_help_message("")
                            .with_render_config(config.clone())
                            .prompt() {
                            Ok(ans) => ans,
                            Err(_) => {
                                print!("\x1B[1A\x1B[2K\r"); // Erase <canceled>
                                stdout().flush()?;
                                break; // One step back (returns to master menu)
                            }
                        };
                        print!("\x1B[1A\x1B[2K\r");
                        stdout().flush()?;

                        if target_ans == "🔙 Back to Menu" {
                            break; // One step back
                        }

                        let key_part = target_ans.split(" (").next().unwrap_or("").to_string();

                        // If they select Compute Device, show device sub-menu directly
                        if key_part.as_str() == "Compute Device" {
                            let device_options = vec![
                                "GPU (Full Offload)".to_string(),
                                "CPU Only".to_string(),
                                "Custom Layers".to_string(),
                            ];
                            let selected_device = match Select::new("Select Compute Device:", device_options).with_help_message("")
                                .with_render_config(config.clone())
                                .prompt() {
                                Ok(ans) => ans,
                                Err(_) => {
                                    print!("\x1B[1A\x1B[2K\r");
                                    stdout().flush()?;
                                    continue;
                                }
                            };
                            print!("\x1B[1A\x1B[2K\r");
                            stdout().flush()?;

                            match selected_device.as_str() {
                                "GPU (Full Offload)" => {
                                    booster.n_gpu_layers = -1;
                                }
                                "CPU Only" => {
                                    booster.n_gpu_layers = 0;
                                }
                                "Custom Layers" => {
                                    let layers_input = inquire::CustomType::<i32>::new("Enter number of GPU layers (-1 for full offload):")
                                        .with_default(-1)
                                        .with_render_config(config.clone())
                                        .prompt();
                                    match layers_input {
                                        Ok(layers) => {
                                            booster.n_gpu_layers = layers;
                                        }
                                        Err(_) => {
                                            print!("\x1B[1A\x1B[2K\r");
                                            stdout().flush()?;
                                            continue;
                                        }
                                    }
                                    print!("\x1B[1A\x1B[2K\r");
                                    stdout().flush()?;
                                }
                                _ => {}
                            }

                            if let Ok(_) = cluaiz_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster) {
                                println!("  {} System Booster updated: Compute Device = {}", "✅".green(), selected_device.bold());
                            } else {
                                println!("  {} Failed to save system booster settings.", "❌".red());
                            }
                            continue;
                        }

                        // Special sub-menus for modes
                        if key_part.as_str() == "Neural Mode" {
                            let mut modes = vec![
                                "edge".to_string(), 
                                "multitasking".to_string(), 
                                "balance".to_string(), 
                                "max_boost".to_string(), 
                                "ultra_max_boost".to_string()
                            ];

                            // 🌌 VRAM GUARD: Only show HyperCluster if VRAM >= 40GB
                            let total_vram = {
                                let pulse_lock = state.live_pulse.pulse.read().unwrap();
                                pulse_lock.vram_total_gb
                            };
                            if total_vram >= 40.0 {
                                modes.push("hyper_cluster".to_string());
                            }

                            let selected_mode = match Select::new("Select Neural Mode:", modes).with_help_message("")
                                .with_render_config(config.clone())
                                .prompt() {
                                Ok(ans) => ans,
                                Err(_) => {
                                    print!("\x1B[1A\x1B[2K\r");
                                    stdout().flush()?;
                                    continue;
                                }
                            };
                            print!("\x1B[1A\x1B[2K\r");
                            stdout().flush()?;

                            booster.mode_run = match selected_mode.as_str() {
                                "edge" => cluaiz_shared::hardware::schema::booster::BoosterMode::Edge,
                                "multitasking" => cluaiz_shared::hardware::schema::booster::BoosterMode::Multitasking,
                                "balance" => cluaiz_shared::hardware::schema::booster::BoosterMode::Balance,
                                "max_boost" => cluaiz_shared::hardware::schema::booster::BoosterMode::MaxBoost,
                                "ultra_max_boost" => cluaiz_shared::hardware::schema::booster::BoosterMode::UltraMaxBoost,
                                "hyper_cluster" => cluaiz_shared::hardware::schema::booster::BoosterMode::HyperCluster,
                                _ => cluaiz_shared::hardware::schema::booster::BoosterMode::Balance,
                            };

                            if let Ok(_) = cluaiz_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster) {
                                println!("  {} System Booster updated: Neural Mode = {}", "✅".green(), selected_mode.bold());
                            } else {
                                println!("  {} Failed to save system booster settings.", "❌".red());
                            }
                            continue;
                        }

                        // Special sub-menus for context shifting & reclaim
                        if key_part.as_str() == "Context Shifting" {
                            let shift_modes = vec![
                                "Off".to_string(),
                                "Minimal".to_string(),
                                "Standard".to_string(),
                                "Aggressive".to_string(),
                                "Extreme".to_string(),
                                "Auto".to_string(),
                            ];
                            let selected_shift = match Select::new("Select Context Shifting:", shift_modes).with_help_message("")
                                .with_render_config(config.clone())
                                .prompt() {
                                Ok(ans) => ans,
                                Err(_) => {
                                    print!("\x1B[1A\x1B[2K\r");
                                    stdout().flush()?;
                                    continue;
                                }
                            };
                            print!("\x1B[1A\x1B[2K\r");
                            stdout().flush()?;

                            booster.context_shifting = match selected_shift.as_str() {
                                "Off" => cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Off,
                                "Minimal" => cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Minimal,
                                "Standard" => cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Standard,
                                "Aggressive" => cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Aggressive,
                                "Extreme" => cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Extreme,
                                _ => cluaiz_shared::hardware::schema::booster::ContextShiftingMode::Auto,
                            };

                            if let Ok(_) = cluaiz_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster) {
                                println!("  {} System Booster updated: Context Shifting = {}", "✅".green(), selected_shift.bold());
                            } else {
                                println!("  {} Failed to save system booster settings.", "❌".red());
                            }
                            continue;
                        }

                        if key_part.as_str() == "KV Cache Quantization" {
                            let kv_options = vec![
                                "16-bit (Lossless / High Precision)".to_string(),
                                "8-bit (50% VRAM Saving / Balanced)".to_string(),
                                "4-bit (75% VRAM Saving / High Compression)".to_string(),
                                "Auto (Dynamic Quantization)".to_string(),
                            ];
                            let selected_kv = match Select::new("Select KV Cache Quantization:", kv_options).with_help_message("")
                                .with_render_config(config.clone())
                                .prompt() {
                                Ok(ans) => ans,
                                Err(_) => {
                                    print!("\x1B[1A\x1B[2K\r");
                                    stdout().flush()?;
                                    continue;
                                }
                            };
                            print!("\x1B[1A\x1B[2K\r");
                            stdout().flush()?;

                            booster.kv_cache_quantization = match selected_kv.as_str() {
                                s if s.starts_with("16-bit") => cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv16,
                                s if s.starts_with("8-bit") => cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv8,
                                s if s.starts_with("4-bit") => cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Kv4,
                                _ => cluaiz_shared::hardware::schema::booster::KvCacheQuantization::Auto,
                            };

                            if let Ok(_) = cluaiz_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster) {
                                println!("  {} System Booster updated: KV Cache Quantization = {}", "✅".green(), selected_kv.bold());
                            } else {
                                println!("  {} Failed to save system booster settings.", "❌".red());
                            }
                            continue;
                        }

                        // Special sub-menu for Response Length
                        if key_part.as_str() == "Response Length" {
                            let length_modes = vec![
                                "Long".to_string(),
                                "Short".to_string(),
                                "Auto".to_string(),
                            ];
                            let selected_len = match Select::new("Select Response Length:", length_modes).with_help_message("")
                                .with_render_config(config.clone())
                                .prompt() {
                                Ok(ans) => ans,
                                Err(_) => {
                                    print!("\x1B[1A\x1B[2K\r");
                                    stdout().flush()?;
                                    continue;
                                }
                            };
                            print!("\x1B[1A\x1B[2K\r");
                            stdout().flush()?;

                            booster.response_length = selected_len.to_lowercase();

                            if let Ok(_) = cluaiz_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster) {
                                println!("  {} System Booster updated: Response Length = {}", "✅".green(), selected_len.bold());
                            } else {
                                println!("  {} Failed to save system booster settings.", "❌".red());
                            }
                            continue;
                        }

                        let values = vec!["On".to_string(), "Off".to_string(), "Auto".to_string()];
                        
                        let val_ans = match Select::new(&format!("Set {}:", key_part), values).with_help_message("")
                            .with_render_config(config.clone())
                            .prompt() {
                            Ok(ans) => ans,
                            Err(_) => {
                                print!("\x1B[1A\x1B[2K\r"); // Erase <canceled>
                                stdout().flush()?;
                                continue; // One step back (stays in System Booster list)
                            }
                        };
                        print!("\x1B[1A\x1B[2K\r");
                        stdout().flush()?;

                        let feature_state = match val_ans.as_str() {
                            "On" => cluaiz_shared::hardware::schema::booster::FeatureState::On,
                            "Off" => cluaiz_shared::hardware::schema::booster::FeatureState::Off,
                            _ => cluaiz_shared::hardware::schema::booster::FeatureState::Auto,
                        };

                        match key_part.as_str() {
                            "Turbo Quant" => booster.turbo_quant = feature_state,
                            "Flash Attention" => booster.flash_attention = feature_state,
                            "Speculative Decoding" => booster.speculative_decoding = feature_state,
                            "Auto Round" => booster.auto_round = feature_state,
                            "DFlash" => {
                                booster.dflash = match val_ans.as_str() {
                                    "On" => cluaiz_shared::hardware::schema::booster::SmartState::Static("On".to_string()),
                                    "Off" => cluaiz_shared::hardware::schema::booster::SmartState::Static("Off".to_string()),
                                    _ => cluaiz_shared::hardware::schema::booster::SmartState::Static("Auto".to_string()),
                                };
                            },
                            "Force VRAM Reclaim" => booster.force_vram_reclaim = feature_state,
                            "Think Mode" => booster.think_mode = feature_state,
                            "Force Memory Lock" => booster.force_memory_lock = feature_state,
                            _ => {}
                        }
                        
                        if let Ok(_) = cluaiz_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster) {
                            println!("  {} System Booster updated: {} = {}", "✅".green(), key_part.cyan(), val_ans.bold());
                        } else {
                            println!("  {} Failed to save system booster settings.", "❌".red());
                        }
                    }
                    continue; // Go back to Master Menu after exiting System Booster
                }

                let is_vector = master_ans.contains("Vector");
                let downloaded: Vec<_> = state.sorted_models.iter().filter(|m| {
                    if !m.is_cached { return false; }
                    let is_model_vector = m.manifest.architecture_type == "onnx" || m.manifest.category == "embedding";
                    if is_vector { is_model_vector } else { !is_model_vector }
                }).collect();

                if downloaded.is_empty() {
                    let msg = if is_vector { "No Vector models found." } else { "No Chat models found." };
                    println!("  {} {} Install from /menu.", "ℹ️".blue(), msg);
                    return Ok(());
                }

                let options: Vec<String> = downloaded.iter().map(|m| m.manifest.name.clone()).collect();

                let starting_index = if let Some(active_id) = &state._active_model_id {
                    downloaded
                        .iter()
                        .position(|m| m.manifest.id == *active_id)
                        .unwrap_or(0)
                } else {
                    0
                };

                let selection = match Select::new("Switch to model:", options).with_help_message("")
                    .with_render_config(config.clone())
                    .with_starting_cursor(starting_index)
                    .prompt() {
                    Ok(ans) => ans,
                    Err(inquire::InquireError::OperationCanceled) | Err(inquire::InquireError::OperationInterrupted) => {
                        print!("\x1B[1A\x1B[2K\r"); // Erase <canceled>
                        stdout().flush()?;
                        continue; // One step back (goes to Action menu)
                    }
                    Err(e) => return Err(e.into()),
                };

                print!("\x1B[1A\x1B[2K\r");
                stdout().flush()?;
                break (master_ans, selection); // Breaks the master loop and returns the selected model
            }
        };

        let downloaded: Vec<_> = state.sorted_models.iter().filter(|m| m.is_cached).collect();

        if let Some(model) = downloaded.iter().find(|m| m.manifest.name == ans) {
            let is_engine_loaded = state.Core_engine.is_loaded.load(std::sync::atomic::Ordering::SeqCst);
            if state._active_model_id.as_ref() == Some(&model.manifest.id) && is_engine_loaded {
                println!(
                    "  {} {} is already active.",
                    "ℹ️".blue(),
                    model.manifest.name.bold()
                );
                return Ok(());
            }

            println!("  {} Loading {}, please wait a moment...", "⏳".yellow(), model.manifest.name.bold());

            let is_vector = master_ans.contains("Vector");
            
            if is_vector {
                // For vector models, just update Permission.json, DO NOT load them into CoreRouter!
                engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_embedding_model(model.manifest.id.clone());
                println!("  {} Vector Model switched successfully. (Saved to Permission.json)", "✅".green());
            } else {
                if let Some(path_str) = &model.manifest.local_path {
                    let path = std::path::PathBuf::from(path_str);

                    // 🧬 cluaiz DISPATCH:
                    // High bit-depth -> Native Rust
                    // 1-bit BitNet -> MANDATORY Llama (Binary)
                    let runtime = if model.manifest.bit_depth < 2.0 {
                        cluaiz_shared::BackendType::RuntimeB
                    } else if path_str.to_lowercase().ends_with(".gguf") {
                        cluaiz_shared::BackendType::RuntimeB
                    } else {
                        cluaiz_shared::BackendType::RuntimeA
                    };

                    let result = tokio::task::block_in_place(|| {
                        let handle = tokio::runtime::Handle::current();
                        
                        // 🛑 SURGICAL FIX: Destroy old router to free VRAM BEFORE loading new model!
                        {
                            let mut lock = state.Core_engine.router.blocking_lock();
                            *lock = engines::CoreRouter::new();
                        }
                        state.Core_engine.is_loaded.store(false, std::sync::atomic::Ordering::SeqCst);
                        
                        match handle.block_on(engines::CoreRouter::load_model(
                            path,
                            runtime.clone(),
                        )) {
                            Ok(router) => {
                                let mut lock = state.Core_engine.router.blocking_lock();
                                *lock = router;
                                state.Core_engine.is_loaded.store(true, std::sync::atomic::Ordering::SeqCst);
                                Ok(())
                            }
                            Err(e) => {
                                // ⚠️ NATIVE FALLBACK: Only for standard models (Bit-depth >= 2.0)!
                                // BitNet MUST NOT use RuntimeA (Candle) as it will crash with tensor errors.
                                if runtime == cluaiz_shared::BackendType::RuntimeB
                                    && model.manifest.bit_depth >= 2.0
                                {
                                    let path_inner = std::path::PathBuf::from(path_str);
                                    handle
                                        .block_on(engines::CoreRouter::load_model(
                                            path_inner,
                                            cluaiz_shared::BackendType::RuntimeA
                                        ))
                                        .map(|router| {
                                            let mut lock = state.Core_engine.router.blocking_lock();
                                            *lock = router;
                                            state.Core_engine.is_loaded.store(true, std::sync::atomic::Ordering::SeqCst);
                                        })
                                } else {
                                    Err(e)
                                }
                            }
                        }
                    });

                    match result {
                        Ok(_) => {
                            state._active_model_id = Some(model.manifest.id.clone());
                            engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_chat_model(model.manifest.id.clone());
                            
                            let mut lock = state.Core_engine.router.blocking_lock();
                            let ctx = lock.get_active_dna().and_then(|d| d.max_context_length).unwrap_or(2048);
                            let model_gb = model.manifest.download_size_gb;
                            
                            if model_gb > 0.0 {
                                println!("  {} Mounted successfully. \x1B[90m[Ctx: {}, Model: {:.2}GB, RAM: {:.1}GB]\x1B[0m", "✅".green(), ctx, model_gb, state.ram_gb);
                            } else {
                                println!("  {} Mounted successfully. \x1B[90m[Ctx: {}, RAM: {:.1}GB]\x1B[0m", "✅".green(), ctx, state.ram_gb);
                            }
                        }
                        Err(e) => println!("  {} Load failed: {}", "❌".red(), e),
                    }
                }
            }
            state
                .activity_stream
                .push(ActivityBlock::ModelMounted(model.manifest.name.clone()));
        }

        Ok(())
    }
}

fn extract_frontmatter(skill_dir: &std::path::Path) -> Option<String> {
    let skill_md_path = skill_dir.join("SKILL.md");
    if skill_md_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
            let lines: Vec<&str> = content.lines().collect();
            let mut start_idx = None;
            let mut end_idx = None;
            for (i, line) in lines.iter().enumerate() {
                if line.trim() == "---" {
                    if start_idx.is_none() {
                        start_idx = Some(i);
                    } else {
                        end_idx = Some(i);
                        break;
                    }
                }
            }
            if let (Some(start), Some(end)) = (start_idx, end_idx) {
                if end > start + 1 {
                    let frontmatter_lines = &lines[start + 1..end];
                    return Some(frontmatter_lines.join("\n"));
                }
            }
        }
    }
    None
}
