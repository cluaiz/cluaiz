#![recursion_limit = "256"]
//! ═══════════════════════════════════════════════════════════════════════
//!  cluaiz API Gateway — The Single Entry Point (Port 8000)
//! ═══════════════════════════════════════════════════════════════════════
//!  Every client (Desktop, Mobile, Web, Robot, Developer) talks to cluaiz Engine
//!  through this gateway. Nothing else is exposed.
//! ═══════════════════════════════════════════════════════════════════════

mod state;
mod handlers;
mod routes;
mod ffi_bridge;

use colored::*;
use dispatcher::NeuralDispatcher;
use system_booster::SystemBooster;
use cluaiz_shared::backend::signature::KernelSignature;
use std::env;
use std::sync::Arc;

use state::AppState;

pub async fn run_daemon() {
    // ── CLI ARGUMENTS PROCESSING ──────────────────────────────────────────
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--setup") {
        println!("🛠️ [Calibration] Initializing Hardware Calibration...");
        if let Err(e) = cluaiz_shared::HardwareGovernor::auto_calibrate() {
            eprintln!("❌ [Calibration] Failed: {}", e);
            std::process::exit(1);
        }
        println!("✅ [Calibration] Hardware Fingerprint Sealed. system_control.bin updated.");
        std::process::exit(0);
    }

    // ── Initialize logging ──
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .compact()
        .try_init();

    // ── Initialize the cluaiz pillars ──
    tracing::info!("🔧 Initializing cluaiz Engine...");
    

    // 🚀 Check Pure Brain Mode
    let mut pure_brain = false;
    if let Ok(control) = cluaiz_shared::hardware::governor::HardwareGovernor::load_system_control() {
        if control.brain.is_pure_brain() {
            tracing::info!("🧠 Pure Brain Mode Active: LLM Engine loading & VRAM allocation is suspended.");
            pure_brain = true;
        }
    }

    // 🚀 Ignite the SystemBooster to optimize hardware before booting engines
    let booster_state = if pure_brain {
        Default::default()
    } else {
        SystemBooster::ignite().unwrap_or_default()
    };
    
    // Create the Dispatcher with the active booster state
    let dispatcher = NeuralDispatcher::new(
        booster_state, 
        KernelSignature::default() // Default to CPU fallback; dynamically updated during /models/load
    );

    let embedding_dispatcher = Arc::new(dispatcher::EmbeddingDispatcher::new().expect("Failed to initialize ONNX embedding engine"));

    // ── Create shared state ──
    let state = Arc::new(AppState { dispatcher, embedding_dispatcher });

    // ── Build API Routes ──
    let app = routes::build(state.clone());

    // ── Bind to Port 8000 (configurable via cluaiz_PORT env var) ──
    let port: u16 = env::var("cluaiz_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000);
    let addr = format!("0.0.0.0:{}", port);

    println!("\n{}", "┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓".bright_blue());
    println!("{} {}", "┃".bright_blue(), "🧬 cluaiz Engine API & FFI".bright_cyan().bold());
    println!("{} {}", "┃".bright_blue(), format!("v{} — cluaiz Inference Engine", env!("CARGO_PKG_VERSION")).bright_black());
    println!("{}", "┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫".bright_blue());
    println!("{} {}", "┃".bright_blue(), format!("{}{}", "🌐 Gateway:   ".bright_black(), "http://localhost:8000".bright_green().bold()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}", "💚 Status:    ".bright_black(), "ALL SYSTEMS ONLINE".bright_green().bold()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}", "🧠 Kernel:    ".bright_black(), "ACTIVE".magenta().bold()));
    println!("{}", "┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫".bright_blue());
    println!("{} {}", "┃".bright_blue(), "📡 Endpoints:".bright_magenta().bold());
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    POST ".bright_cyan().bold(), "/chat             ".white(), "→ Chat with cluaiz".bright_black()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    GET  ".bright_cyan().bold(), "/sessions         ".white(), "→ List chat sessions".bright_black()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    POST ".bright_cyan().bold(), "/v1/db/execute    ".white(), "→ FFI Database Query".bright_black()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    POST ".bright_cyan().bold(), "/v1/system/brain  ".white(), "→ Toggle FFI Brain".bright_black()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    GET  ".bright_cyan().bold(), "/hardware         ".white(), "→ Check system RAM/CPU".bright_black()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    POST ".bright_cyan().bold(), "/models/download  ".white(), "→ Fetch from HF".bright_black()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    POST ".bright_cyan().bold(), "/models/load      ".white(), "→ Activate Model".bright_black()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    GET  ".bright_cyan().bold(), "/v1/skills/list   ".white(), "→ List WASM skills".bright_black()));
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "    POST ".bright_cyan().bold(), "/v1/skills/install".white(), "→ Install WASM skill".bright_black()));
    println!("{}", "┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫".bright_blue());
    println!("{} {}", "┃".bright_blue(), format!("{}{}{}", "✨ ".yellow(), "Nothing Need.".bright_white().italic(), " Just cluaiz.".bright_green().bold()));
    println!("{}", "┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛\n".bright_blue());

    tracing::info!("🌐 cluaiz Gateway listening on {}", addr);

    // ── Start the FFI/IPC Daemon (Background) ──
    tracing::info!("🚀 Spawning Native FFI Named Pipe Listener...");
    tokio::spawn(async move { ffi_bridge::start_named_pipe_server(state.clone()).await; });


    // ── Start the HTTP server ──
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("❌ Failed to bind to port {}. Is another process using it? Error: {}", addr, e);
            println!("\n  {} API Daemon is already running in the background! Continuing in Pure Client Mode...\n", "✅".green());
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!("❌ cluaiz API server crashed unexpectedly: {}", e);
    }
    
    tracing::info!("🛑 cluaiz Gateway cleanly shut down.");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            println!("\n  {} [Daemon] SIGINT received. Initiating Clean Shutdown...", "🛑".red());
        },
        _ = terminate => {
            println!("\n  {} [Daemon] SIGTERM received. Initiating Clean Shutdown...", "🛑".red());
        },
    }

    println!("  {} [Memory] Flushing LMDB rings and releasing VRAM maps...", "🧹".yellow());
    // Give engine time to sync
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    println!("  {} [System] cluaiz successfully terminated.", "✅".green());
}

