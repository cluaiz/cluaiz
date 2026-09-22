
pub mod models_runner;
pub mod system_control_manager;

// Re-exporting from engine_core/hardware/schema
pub use engine_core::hardware::schema::profiles::{
    SiliconTruth, 
    MemorySubsystem, 
    StorageSubsystem, 
    CpuSubsystem,
    Accelerators
};
pub use engine_core::hardware::schema::metrics::SiliconMetrics;

pub struct HardwareDetector;
impl Default for HardwareDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareDetector {
    pub fn new() -> Self { Self }

    /// 🏛️ Executes the physical hardware detection protocol.
    pub fn detect(&self) -> SiliconTruth {
        system_control_manager::detect_hardware()
    }
}

pub enum InferenceEngine {
    cluaiz,
    Llama,
    Candle,
}

pub enum InferenceEvent {
    Started,
    Progress(f32),
    Completed,
    Failed(String),
}
