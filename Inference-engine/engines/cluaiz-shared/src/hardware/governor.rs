use crate::hardware::schema::booster::BoosterControl;
use crate::hardware::schema::profiles::SystemControl;
use crate::hardware::system_control::HardwareOrchestrator;
use once_cell::sync::Lazy;
use rkyv::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// 🧠 VRAM Arbiter State: Tracks real-time resource allocations.
pub struct ArbiterState {
    pub total_vram_gb: f64,
    pub allocated_vram_gb: f64,
    pub active_allocations: HashMap<String, f64>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub model_id: String,
    pub vram_gb: f64,
    pub context_size: usize,
    pub engine: String,
}

static ARBITER: Lazy<Mutex<ArbiterState>> = Lazy::new(|| {
    Mutex::new(ArbiterState {
        total_vram_gb: 0.0,
        allocated_vram_gb: 0.0,
        active_allocations: HashMap::new(),
    })
});

#[derive(Clone, Copy, Default)]
pub struct HardwareGovernor;

impl HardwareGovernor {
    /// 🚀 Initialize the Governor and resolve hardware state.
    pub fn start() -> Self {
        Self
    }

    /// 🛡️ Checks if the 'system_control.json' fingerprint exists.
    pub fn is_ready(&self) -> bool {
        Self::resolve_engine_path()
            .join("system_control.json")
            .exists()
    }

    /// 🔬 Deep surgical scan and persistence of silicon state.
    pub fn auto_calibrate() -> anyhow::Result<()> {
        let control = HardwareOrchestrator::start()?;
        Self::save_booster_settings(&Self::load_booster_settings().unwrap_or_default())?;

        // 🧠 Mission 12: Chronicle Foundry State
        let _ = crate::neural::graph::NeuralGraph::chronicle_pulse(
            "Foundry Calibration & Silicon Audit",
            "HardwareGovernor",
            &format!(
                "Silicon: {}, Arch: {}",
                control.silicon_truth.cpu.brand.trim(),
                control.identity.architecture
            ),
        );

        // Update Arbiter with latest hardware truth
        if let Ok(mut arbiter) = ARBITER.lock() {
            let total = control
                .silicon_truth
                .accelerators
                .gpus
                .iter()
                .map(|g| g.vram_available_gb)
                .sum::<f64>();
            arbiter.total_vram_gb = total;
        }

        Ok(())
    }

    /// ⚖️ Request VRAM allocation for a neural engine.
    /// Prevents OOM by enforcing the sovereign memory budget.
    pub fn request_vram(engine_id: &str, required_gb: f64) -> anyhow::Result<()> {
        let mut arbiter = ARBITER
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter Lock Poisoned"))?;

        // If total_vram is 0, we try to load from the existing System Truth first (Fast)
        if arbiter.total_vram_gb == 0.0 {
            if let Ok(control) = Self::load_system_control() {
                let total = control
                    .silicon_truth
                    .accelerators
                    .gpus
                    .iter()
                    .map(|g| g.vram_total_gb)
                    .sum::<f64>();
                arbiter.total_vram_gb = total;
                tracing::info!(
                    "⚖️ [Arbiter] VRAM Truth synchronized from System Control (Total): {:.2}GB",
                    total
                );
            } else {
                // Only calibrate if absolutely no truth is found (Slow fallback)
                let _ = Self::auto_calibrate();
            }
        }

        // 🛡️ Sovereign Limit: In Max Boost, we allow 98% utilization.
        let booster = Self::load_booster_settings().unwrap_or_default();
        let safety_margin = match booster.mode_run {
            crate::hardware::schema::booster::BoosterMode::UltraMaxBoost
            | crate::hardware::schema::booster::BoosterMode::HyperCluster => 0.025, // 2.5% Margin for extreme modes (102MB on 4GB)
            crate::hardware::schema::booster::BoosterMode::MaxBoost => 0.05, // 5% Margin
            crate::hardware::schema::booster::BoosterMode::Balance => 0.07,  // 7% Margin
            _ => 0.12,                                                       // 12% for multitasking
        };

        let available = arbiter.total_vram_gb * (1.0 - safety_margin) - arbiter.allocated_vram_gb;

        if required_gb > available {
            crate::dev_info!(
                "❌ [VRAM Arbiter] Out of Memory! Requested: {:.2}GB, Available: {:.2}GB (Limit: {:.0}% Utilization)",
                required_gb, available, (1.0 - safety_margin) * 100.0
            );
            return Err(anyhow::anyhow!(
                "❌ [VRAM Arbiter] Out of Memory! Requested: {:.2}GB, Available: {:.2}GB",
                required_gb, available
            ));
        }

        // Allocate
        arbiter.allocated_vram_gb += required_gb;
        arbiter
            .active_allocations
            .insert(engine_id.to_string(), required_gb);

        crate::dev_info!(
            "✅ [VRAM Arbiter] Allocated {:.2}GB to '{}'. Current Load: {:.2}/{:.2}GB",
            required_gb, engine_id, arbiter.allocated_vram_gb, arbiter.total_vram_gb
        );

        // Sync to cross-process registry
        let mut registry = Self::load_process_registry();
        registry.insert(
            std::process::id().to_string(),
            ProcessInfo {
                pid: std::process::id(),
                model_id: engine_id.to_string(),
                vram_gb: required_gb,
                context_size: 0, // Will be updated by negotiate_vram_envelope
                engine: "Native Llama".to_string(),
            },
        );
        Self::save_process_registry(&registry);

        Ok(())
    }

    /// ⚖️ Negotiate VRAM Envelope: Performs an iterative fitting loop
    /// to find the maximum safe context window for the current silicon state.
    /// This is NO LONGER static; it recalculates based on live architecture and booster state.
    pub fn negotiate_vram_envelope(dna: &crate::metadata::dna::StructuralDNA) -> usize {
        let booster = Self::load_booster_settings().unwrap_or_default();
        Self::negotiate_vram_envelope_with_booster(dna, &booster)
    }

    pub fn negotiate_vram_envelope_with_booster(
        dna: &crate::metadata::dna::StructuralDNA,
        booster: &crate::hardware::schema::booster::BoosterControl,
    ) -> usize {
        let mut arbiter = ARBITER.lock().unwrap();

        let path = Self::resolve_engine_path().join("config").join("system_booster.json");

        // 🔍 LIVE SILICON PROBE: We don't trust cached values for safety-critical negotiation.
        if let Ok(control) = Self::load_system_control() {
            arbiter.total_vram_gb = control
                .silicon_truth
                .accelerators
                .gpus
                .iter()
                .map(|g| g.vram_total_gb)
                .sum::<f64>();
        } else if arbiter.total_vram_gb == 0.0 {
            let _ = Self::auto_calibrate();
        }

        // 🌊 ADAPTIVE MARGIN LOGIC: No more static percentages.
        // We scale the margin based on total VRAM to prevent waste on H100s and starvation on 2GB cards.
        let total_gb = arbiter.total_vram_gb;
        let mut margin = match booster.mode_run {
            crate::hardware::schema::booster::BoosterMode::Edge => 0.05f64.min(0.2 / total_gb), // 📱 Max 5% or 200MB
            crate::hardware::schema::booster::BoosterMode::Multitasking => {
                0.30f64.min(1.5 / total_gb)
            } // 💻 Max 30% or 1.5GB
            crate::hardware::schema::booster::BoosterMode::Balance => 0.15f64.min(2.0 / total_gb), // ⚖️ Max 15% or 2GB
            crate::hardware::schema::booster::BoosterMode::MaxBoost => 0.10f64.max(0.6 / total_gb), // 🚀 Safe Aggressive: 600MB Margin (Zero Spill)
            crate::hardware::schema::booster::BoosterMode::UltraMaxBoost => {
                0.01f64.max(0.25 / total_gb)
            } // 🔥 Absolute Limit: 250MB Margin (Maximum Context, Risk of Spill)
            crate::hardware::schema::booster::BoosterMode::HyperCluster => {
                if total_gb < 40.0 {
                    println!("⚠️ [Arbiter] VRAM GUARD: HyperCluster rejected (<40GB). Falling back to UltraMaxBoost.");
                    0.01f64.max(0.25 / total_gb) // Fallback to UltraMaxBoost safety
                } else {
                    0.15 // 🌌 Server (True Zero-ish)
                }
            }
        };

        // 🛡️ FLOOR SAFETY: Ensure OS always has breathing room.
        let floor_gb = if matches!(
            booster.mode_run,
            crate::hardware::schema::booster::BoosterMode::UltraMaxBoost
                | crate::hardware::schema::booster::BoosterMode::HyperCluster
        ) {
            0.25 // 250MB Absolute Minimum
        } else {
            0.60 // 600MB Standard Safety Floor
        };

        // Apply floor unless we are on massive server hardware (>24GB)
        if total_gb < 24.0 {
            margin = margin.max(floor_gb / total_gb);
        }

        // 🔥 'UltraMax' Override: Extreme utilization but still respects our new floor
        if booster.force_vram_reclaim == crate::hardware::schema::booster::FeatureState::On {
            let tight_margin = 0.005f64; // 0.5%
            margin = tight_margin.max(floor_gb / total_gb);
        }

        // We use static theoretical math for context negotiation.
        // Using live_vram_probe() here squashes the context window on subsequent prompts
        // because the context is already allocated in VRAM, making live VRAM appear artificially low.
        let other_allocations = arbiter
            .active_allocations
            .iter()
            .filter(|(id, _)| {
                !id.contains(&dna.model_identity)
                    && id.as_str() != "llama"
                    && id.as_str() != "onnx"
                    && id.as_str() != "whisper"
            })
            .map(|(_, gb)| gb)
            .sum::<f64>();
        let available_gb = (total_gb * (1.0 - margin)) - other_allocations;
        let final_available_gb = (available_gb - (dna.weights_size_gb as f64)).max(0.0);

        // 🧪 SOVEREIGN MATH: Calculate KV-Cache cost per 1024 tokens for THIS model
        let layers = dna.layer_count.unwrap_or(32) as f64;
        let kv_heads = dna
            .attention_head_count_kv
            .or(dna.attention_head_count)
            .unwrap_or(32) as f64;

        // 🧬 DNA Interrogation: head_dim = hidden_size / heads (Architecture Truth)
        let head_dim_calc = if let (Some(h), Some(c)) = (dna.hidden_size, dna.attention_head_count)
        {
            (h / c) as f64
        } else {
            dna.attention_head_dim.unwrap_or(128) as f64
        };

        let head_dim = dna
            .attention_head_dim
            .map(|d| d as f64)
            .unwrap_or(head_dim_calc);

        // 🚀 Conservative Math: Always assume FP16 for KV-cache unless confirmed by engine state.
        let bytes_per_element = 2.0; // FP16 standard (Safe)

        // GB per 1024 tokens
        let gb_per_k = (1024.0 * layers * kv_heads * head_dim * bytes_per_element * 2.0)
            / (1024.0 * 1024.0 * 1024.0);

        // 🛑 DYNAMIC STABILITY CAP: No more static traps.
        // Rule: Never exceed what the model architecture supports (DNA Truth).
        // If DNA is missing, we assume an infinite architecture limit (usize::MAX)
        // and let the Physical VRAM Arbiter determine the safe ceiling.
        let arch_cap = dna.max_context_length.unwrap_or(usize::MAX);

        // If CPU-only Mode (n_gpu_layers = 0), bypass GPU VRAM constraints entirely
        if booster.n_gpu_layers == 0 {
            let cpu_ctx = if arch_cap == usize::MAX {
                32000
            } else {
                arch_cap
            };
            println!("⚖️ [Arbiter] CPU-only Mode detected (n_gpu_layers = 0). Bypassing GPU VRAM constraints. Safe Context: {} tokens", cpu_ctx);

            let mut registry = Self::load_process_registry();
            let pid_str = std::process::id().to_string();
            if let Some(info) = registry.get_mut(&pid_str) {
                info.context_size = cpu_ctx;
                Self::save_process_registry(&registry);
            }
            return cpu_ctx;
        }

        // Starting point for negotiation should be the Architecture Truth
        let mut current_ctx = arch_cap;

        // Expansion logic for high-power modes (Only if architecture allows)
        // 🚀 THE REALITY DOCTRINE (CERD): 3-Tier Hardware Modes
        let is_hybrid_requested = booster.force_vram_reclaim == crate::hardware::schema::booster::FeatureState::On;
        let model_exceeds_vram = (dna.weights_size_gb as f64) > (arbiter.total_vram_gb * (1.0 - margin));

        if is_hybrid_requested || model_exceeds_vram {
            // 🔄 HYBRID MODE (Explicitly requested OR auto-triggered because VRAM is too small)
            // Use VRAM + Shared System RAM to calculate absolute maximum possible context.
            let mut system_ram_gb = 0.0;
            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            let avail_ram_bytes = sys.available_memory(); // ACTUAL FREE RAM
            if avail_ram_bytes > 0 {
                system_ram_gb = (avail_ram_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
            }
            
            let total_combined_gb = arbiter.total_vram_gb + system_ram_gb;
            let safe_combined_gb = (total_combined_gb * (1.0 - margin)).max(0.0);
            let available_for_kv = (safe_combined_gb - (dna.weights_size_gb as f64)).max(0.0);
            
            let max_possible_k = available_for_kv / gb_per_k;
            current_ctx = ((max_possible_k * 1024.0) as usize).min(arch_cap);
        } else {
            // ⚡ GPU ONLY MODE (Default)
            // Model easily fits in VRAM. Give it ONLY the context that fits perfectly in Dedicated VRAM.
            // This guarantees MAX TPS and zero shared memory spill.
            let possible_max = (final_available_gb / gb_per_k) * 1024.0;
            current_ctx = (possible_max as usize).min(arch_cap);
        }

        // Envelope Negotiation Log Hidden for clean UI
        // Sync context size to cross-process registry
        let mut registry = Self::load_process_registry();
        let pid_str = std::process::id().to_string();
        if let Some(info) = registry.get_mut(&pid_str) {
            info.context_size = current_ctx;
            Self::save_process_registry(&registry);
        }

        current_ctx
    }

    /// 🔓 Release VRAM allocation when an engine is unloaded.
    pub fn release_vram(engine_id: &str) -> anyhow::Result<()> {
        let mut arbiter = ARBITER
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter Lock Poisoned"))?;

        if let Some(freed_gb) = arbiter.active_allocations.remove(engine_id) {
            arbiter.allocated_vram_gb -= freed_gb;
            crate::dev_info!(
                "🔓 [VRAM Arbiter] Released {:.2}GB from '{}'. Current Load: {:.2}/{:.2}GB",
                freed_gb, engine_id, arbiter.allocated_vram_gb, arbiter.total_vram_gb
            );
        }

        // Remove from cross-process registry
        let mut registry = Self::load_process_registry();
        registry.remove(&std::process::id().to_string());
        Self::save_process_registry(&registry);

        Ok(())
    }

    /// Load the cross-process registry of active LLMs.
    pub fn load_process_registry() -> HashMap<String, ProcessInfo> {
        let path = Self::resolve_engine_path().join("config").join("active_processes.json");
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    /// Save the cross-process registry.
    pub fn save_process_registry(registry: &HashMap<String, ProcessInfo>) {
        let path = Self::resolve_engine_path().join("config").join("active_processes.json");
        if let Ok(data) = serde_json::to_string_pretty(registry) {
            let _ = std::fs::write(&path, data);
        }
    }

    /// ⚙️ Updates a specific field in the sovereign configuration.
    pub fn update_field(field: &str, value: serde_json::Value) -> anyhow::Result<()> {
        let mut control = HardwareOrchestrator::start()?;

        // ⚙️ Sovereign Configuration Dispatch
        match field {
            "machine_name" => {
                if let Some(s) = value.as_str() {
                    control.identity.machine_name = s.to_string();
                }
            }
            "runtime_engine.booster_flags.TurboQuant_Enable" => {
                let mut booster = Self::load_booster_settings().unwrap_or_default();
                if let Some(b) = value.as_bool() {
                    booster.turbo_quant = if b {
                        crate::hardware::schema::booster::FeatureState::On
                    } else {
                        crate::hardware::schema::booster::FeatureState::Off
                    };
                    Self::save_booster_settings(&booster)?;
                }
            }
            "runtime_engine.booster_flags.FlashAttention_v2" => {
                let mut booster = Self::load_booster_settings().unwrap_or_default();
                if let Some(b) = value.as_bool() {
                    booster.flash_attention = if b {
                        crate::hardware::schema::booster::FeatureState::On
                    } else {
                        crate::hardware::schema::booster::FeatureState::Off
                    };
                    Self::save_booster_settings(&booster)?;
                }
            }
            _ => println!("⚠️ [Governor] Field update NOT implemented: {}", field),
        }

        // Save back the updated control
        let base = Self::resolve_engine_path().join("config");
        let _ = std::fs::create_dir_all(&base);
        let json_data = serde_json::to_string_pretty(&control)?;
        std::fs::write(base.join("system_control.json"), json_data)?;

        Ok(())
    }

    /// Resolves the base Hub directory for cluaiz configurations.
    /// Priority:
    /// 1. cluaiz_ROOT environment variable.
    /// 2. Portable Mode: Parent directory of current executable.
    /// 3. OS Standard Config Dir.
    pub fn resolve_hub_path() -> PathBuf {
        crate::environment::EnvironmentManager::current().local_dir
    }

    pub fn resolve_apps_path() -> PathBuf {
        let path = Self::resolve_hub_path().join("apps");
        let _ = std::fs::create_dir_all(&path);
        path
    }

    pub fn resolve_app_path(name: &str) -> PathBuf {
        let path = Self::resolve_apps_path().join(name);
        let _ = std::fs::create_dir_all(&path);
        path
    }

    pub fn resolve_engine_path() -> PathBuf {
        let path = Self::resolve_hub_path().join("engine");
        let _ = std::fs::create_dir_all(&path);
        path
    }

    pub fn resolve_interface_path() -> PathBuf {
        let path = Self::resolve_engine_path();
        let _ = std::fs::create_dir_all(&path);
        path
    }

    pub fn resolve_booster_path() -> PathBuf {
        let path = Self::resolve_engine_path().join("booster");
        let _ = std::fs::create_dir_all(&path);
        path
    }

    pub fn resolve_vault_path() -> PathBuf {
        let path = Self::resolve_hub_path().join("vault");
        let _ = std::fs::create_dir_all(&path);
        path
    }

    pub fn resolve_modules_path() -> PathBuf {
        let path = Self::resolve_hub_path().join("modules");
        let _ = std::fs::create_dir_all(&path);
        path
    }

    pub fn resolve_bin_gateway() -> PathBuf {
        let path = Self::resolve_hub_path().join("bin");
        std::fs::create_dir_all(&path).expect("Failed to create bin directory");
        path
    }

    // ─── 🚀 SYSTEM CONTROL (BINARY TRUTH) ───

    /// 🏛️ Loads the sovereign hardware fingerprint from the binary truth (.bin).
    /// If missing, it triggers an automatic "Self-Healing" recovery scan.
    pub fn load_binary_truth() -> anyhow::Result<SystemControl> {
        let path = Self::resolve_engine_path().join("config").join("system_control.bin");

        if !path.exists() {
            return Err(anyhow::anyhow!("Binary truth missing"));
        }

        let bytes_raw = std::fs::read(&path)?;
        let mut bytes = rkyv::AlignedVec::with_capacity(bytes_raw.len());
        bytes.extend_from_slice(&bytes_raw);

        // 🛡️ Ultimate Safety Guard: Catch rkyv panics (overflows/alignment)
        let result = std::panic::catch_unwind(|| {
            if bytes.len() < 32 {
                return None;
            }
            let archived = unsafe { rkyv::archived_root::<SystemControl>(&bytes) };
            archived.deserialize(&mut rkyv::Infallible).ok()
        });

        match result {
            Ok(Some(control)) => Ok(control),
            _ => {
                let _ = std::fs::remove_file(&path);
                println!("⚠️ [Self-Healing] Binary Truth Corrupted. Recovering...");
                Self::auto_calibrate()?;
                Err(anyhow::anyhow!("Binary truth recovered. Please retry."))
            }
        }
    }

    pub fn load_system_control() -> anyhow::Result<SystemControl> {
        let base = Self::resolve_engine_path().join("config");
        let path = base.join("system_control.json");
        let bin_path = base.join("system_control.bin");

        if !path.exists() {
            if !bin_path.exists() {
                println!("🛠️ [Self-Healing] System Truth LOST. Initiating Full Recovery...");
                Self::auto_calibrate()?;
            } else {
                return Self::load_binary_truth();
            }
        }

        let data =
            std::fs::read_to_string(&path).map_err(|_| anyhow::anyhow!("JSON Load Failed"))?;
        let control: SystemControl = match serde_json::from_str(&data) {
            Ok(val) => val,
            Err(_) => {
                println!("⚠️ [Self-Healing] JSON Tampered. Restoring from Binary...");
                Self::load_binary_truth().unwrap_or_default()
            }
        };
        Ok(control)
    }

    pub fn save_system_control(control: &SystemControl) -> anyhow::Result<()> {
        let base = Self::resolve_engine_path().join("config");
        std::fs::create_dir_all(&base)?;

        let json_path = base.join("system_control.json");
        let bin_path = base.join("system_control.bin");
        let temp_json = json_path.with_extension("json.tmp");
        let temp_bin = bin_path.with_extension("bin.tmp");

        // ✍️ Atomic Write Protocol: Write to Temp -> Sync -> Rename
        let json_data = serde_json::to_string_pretty(control)?;
        std::fs::write(&temp_json, json_data)?;

        let bytes = rkyv::to_bytes::<_, 4096>(control)
            .map_err(|e| anyhow::anyhow!("Binary Serialization Failed: {}", e))?;
        std::fs::write(&temp_bin, bytes.as_slice())?;

        // Atomic Swap
        std::fs::rename(temp_json, json_path)?;
        std::fs::rename(temp_bin, bin_path)?;

        Ok(())
    }

    // ─── 🚀 BOOSTER CONTROL (USER SETTINGS) ───

    pub fn load_booster_settings() -> anyhow::Result<BoosterControl> {
        let base = Self::resolve_engine_path().join("config");
        let bin_path = base.join("system_booster.bin");
        let json_path = base.join("system_booster.json");

        // 🛡️ Priority 1: JSON (User Editable Truth)
        if json_path.exists() {
            if let Ok(data) = std::fs::read_to_string(&json_path) {
                match serde_json::from_str::<BoosterControl>(&data) {
                    Ok(control) => {
                        // Always sync to binary truth to keep .bin updated in real-time when loaded
                        let _ = Self::save_booster_settings(&control);
                        return Ok(control);
                    }
                    Err(e) => {
                        eprintln!("❌ [Arbiter] Failed to parse system_booster.json: {}. Falling back to binary.", e);
                    }
                }
            }
        }

        // 🚀 Priority 2: Binary Truth (Panic-Safe Rkyv Fallback)
        if bin_path.exists() {
            if let Ok(bytes_raw) = std::fs::read(&bin_path) {
                let mut bytes = rkyv::AlignedVec::with_capacity(bytes_raw.len());
                bytes.extend_from_slice(&bytes_raw);
                {
                    let result = std::panic::catch_unwind(|| {
                        if bytes.len() < 32 {
                            return None;
                        }
                        let archived = unsafe { rkyv::archived_root::<BoosterControl>(&bytes) };
                        archived.deserialize(&mut rkyv::Infallible).ok()
                    });

                    if let Ok(Some(control)) = result {
                        return Ok(control);
                    }
                }
                // If panic or error, wipe it
                let _ = std::fs::remove_file(&bin_path);
            }
        }

        Ok(BoosterControl::default())
    }

    pub fn save_booster_settings(control: &BoosterControl) -> anyhow::Result<()> {
        let base = Self::resolve_engine_path().join("config");
        std::fs::create_dir_all(&base)?;

        let json_path = base.join("system_booster.json");
        let bin_path = base.join("system_booster.bin");

        // 🔓 Sovereign Freedom: Removed Read-Only locking to allow real-time manual edits.

        // ✍️ Write Booster Settings
        let json_data = serde_json::to_string_pretty(control)?;
        std::fs::write(&json_path, json_data)?;

        let bytes = rkyv::to_bytes::<_, 1024>(control)
            .map_err(|e| anyhow::anyhow!("Binary Serialization Failed: {}", e))?;
        std::fs::write(&bin_path, bytes.as_slice())?;

        Ok(())
    }

    /// 🔒 Applies OS-level protection to a file to prevent manual deletion or tampering.
    fn _set_file_lock(path: &std::path::Path, locked: bool) {
        // [DEPRECATED] Sovereign mandated manual control.
        if let Ok(metadata) = std::fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(locked);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
}

/// 🏛️ RegistryGovernor: Manages the Master Ecosystem Registry (package.json + package.bin)
pub struct RegistryGovernor;

impl RegistryGovernor {
    /// Resolves the local path for the master package registry.
    pub fn resolve_registry_path() -> (PathBuf, PathBuf) {
        let engine_dir = HardwareGovernor::resolve_engine_path().join("config");
        (
            engine_dir.join("package.json"),
            engine_dir.join("package.bin"),
        )
    }

    /// 🏛️ Synchronizes the master registry from remote and seals it into binary truth.
    pub fn seal_registry(data: serde_json::Value) -> anyhow::Result<()> {
        let (json_path, bin_path) = Self::resolve_registry_path();
        
        if let Some(parent) = json_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let temp_json = json_path.with_extension("json.tmp");
        let temp_bin = bin_path.with_extension("bin.tmp");

        // ✍️ Atomic Registry Update
        let json_str = serde_json::to_string_pretty(&data)?;
        std::fs::write(&temp_json, json_str)?;
        std::fs::write(&temp_bin, serde_json::to_vec(&data)?)?;

        // Atomic Swap
        std::fs::rename(temp_json, json_path)?;
        std::fs::rename(temp_bin, bin_path)?;

        Ok(())
    }

    /// 🛡️ Loads the latest registry, preferring Binary Truth if JSON is missing/corrupt.
    pub fn load_registry() -> anyhow::Result<serde_json::Value> {
        let (json_path, bin_path) = Self::resolve_registry_path();

        if json_path.exists() {
            let data = std::fs::read_to_string(json_path)?;
            return Ok(serde_json::from_str(&data)?);
        }

        if bin_path.exists() {
            let bytes = std::fs::read(bin_path)?;
            return Ok(serde_json::from_slice(&bytes)?);
        }

        Err(anyhow::anyhow!(
            "Ecosystem Registry LOST. Requires Sovereign Handshake."
        ))
    }

    /// 🧠 Resolve Best Backend: Maps real hardware truth to the best available registry backend.
    pub fn resolve_backend(
        control: &crate::hardware::schema::profiles::SystemControl,
        _registry: &serde_json::Value,
    ) -> String {
        let os = control.identity.os_target.to_lowercase();
        let _arch = control.identity.architecture.to_lowercase();
        let gpu_vendor = control
            .silicon_truth
            .accelerators
            .gpus
            .first()
            .map(|g| g.vendor.to_lowercase())
            .unwrap_or_default();

        // 🚀 Sovereign Routing Strategy:
        // Priority 1: Check if registry has a specific hardware match
        // Priority 2: Fallback to generic platform matching

        if os == "macos" && gpu_vendor.contains("apple") {
            return "metal".to_string();
        }

        if gpu_vendor.contains("nvidia") {
            return "cuda".to_string();
        }

        if gpu_vendor.contains("amd") {
            return "rocm".to_string();
        }

        if gpu_vendor.contains("intel") {
            return "openvino".to_string();
        }

        // Default to CPU-based ISA optimization
        "cpu".to_string()
    }
}
