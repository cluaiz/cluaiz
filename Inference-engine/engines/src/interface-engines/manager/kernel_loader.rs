use std::path::PathBuf;


/// Reads `cluaize_root` securely via the Cluaize Hardware Governor.
/// This uses the binary truth (`system_control.bin`) as the ultimate source,
/// exactly as the Cluaize Architecture intends. Zero custom hardcoding.
fn read_cluaize_root() -> Option<PathBuf> {
    match cluaize_shared::HardwareGovernor::load_system_control() {
        Ok(control) => Some(PathBuf::from(control.context.cluaize_root)),
        Err(e) => {
            tracing::error!("❌ [KernelLoader] Failed to read System Truth: {}", e);
            None
        }
    }
}

/// Kernel Loader
/// Manages pre-compiled binaries (.dll, .so, .dylib) for different OS/Architecture pairs.
/// All paths are resolved dynamically via system_control.json. Zero hardcoding.
pub struct KernelLoader {
    base_dir: PathBuf,
}

impl KernelLoader {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Checks if a kernel binary exists locally for a target OS.
    pub fn exists_for_os(&self, kernel_name: &str, os: &str) -> bool {
        let path = self.resolve_path_for_os(kernel_name, os);
        path.exists()
    }

    /// Checks if a kernel binary exists locally for the current OS.
    pub fn exists(&self, kernel_name: &str) -> bool {
        let path = self.resolve_path(kernel_name);
        path.exists()
    }

    /// Resolves path based on current compilation target (NATIVE).
    pub fn resolve_path(&self, kernel_name: &str) -> PathBuf {
        let os = if cfg!(target_os = "windows") { "Windows" }
            else if cfg!(target_os = "linux") { "Linux" }
            else if cfg!(target_os = "android") { "Android" }
            else if cfg!(target_os = "macos") { "macOS" }
            else if cfg!(target_os = "ios") { "iOS" }
            else { "Unknown" };
        self.resolve_path_for_os(kernel_name, os)
    }

    /// Resolves the absolute path for a kernel binary for a SPECIFIC OS.
    /// Priority: [cluaize_root]/interface-engines/ → fallback to base_dir/target/release/
    pub fn resolve_path_for_os(&self, kernel_name: &str, os: &str) -> PathBuf {
        let ext = match os {
            "Windows" => "dll",
            "Linux" | "Android" => "so",
            "macOS" | "iOS" => "dylib",
            _ => "bin",
        };

        // We try multiple potential naming conventions and subdirectories
        let mut candidates = Vec::new();
        
        // 1. Unified Cluaize Naming Format (e.g. cluaize-llama.dll, libcluaize_llama.so)
        candidates.push(format!("cluaize-{}.{}", kernel_name, ext));
        candidates.push(format!("cluaize_{}.{}", kernel_name, ext));
        candidates.push(format!("libcluaize_{}.{}", kernel_name, ext));
        candidates.push(format!("libcluaize-{}.{}", kernel_name, ext));
        
        // 2. Legacy Archer Naming Format (e.g. archer_llama.dll, libarcher_llama.so)
        candidates.push(format!("archer_{}.{}", kernel_name, ext));
        candidates.push(format!("archer-{}.{}", kernel_name, ext));
        candidates.push(format!("libarcher_{}.{}", kernel_name, ext));
        
        // 3. DEVELOPMENT FALLBACK: Check local target/debug/ if we are running from source.
        // We prioritize this over the global installation so developers don't accidentally load stale DLLs.
        let mut dev_path = if let Some(root) = read_cluaize_root() {
            // We shouldn't use cluaize_root for dev_path because that's the global .cluaize folder.
            // We should use the current working directory where Cargo is building.
            std::env::current_dir().unwrap_or(self.base_dir.clone())
        } else {
            std::env::current_dir().unwrap_or(self.base_dir.clone())
        };
        dev_path.push("target");

        // Always prefer release profile for maximum performance, even in debug builds
        let profiles = ["release", "debug"];

        for profile in &profiles {
            let profile_path = dev_path.join(profile);
            for file_name in &candidates {
                let path = profile_path.join(file_name);
                if path.exists() {
                    tracing::info!("🎯 [KernelLoader] Cluaize dev path resolved ({}): {:?}", profile, path);
                    return path;
                }
            }
        }

        // 4. System Truth: Read cluaize_root (Global Installation)
        if let Some(_) = read_cluaize_root() {
            let env = cluaize_shared::environment::EnvironmentManager::current();
            let base_link = env.engine_dir().join("interfaces");
            
            for file_name in &candidates {
                // Check root interface-engines/
                let path = base_link.join(file_name);
                if path.exists() {
                    tracing::info!("🎯 [KernelLoader] Cluaize path resolved: {:?}", path);
                    return path;
                }
                
                // Check kernels/ subdirectory
                let path_kernels = base_link.join("kernels").join(file_name);
                if path_kernels.exists() {
                    tracing::info!("🎯 [KernelLoader] Cluaize path resolved (kernels/): {:?}", path_kernels);
                    return path_kernels;
                }
            }
        }
        
        tracing::warn!("⚠️ [KernelLoader] Cluaize path not found for {}. Checked dev paths.", kernel_name);
        dev_path.join("release").join(format!("cluaize_{}.{}", kernel_name, ext))
    }
}

