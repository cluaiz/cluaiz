pub mod gguf_prober;
pub use gguf_prober::GGUFProber;
pub mod spinner;
pub mod model_registry;
pub use model_registry::{ModelRegistry, ModelRegistryEntry, RegistryModelFile, RegistryModelMetadata, ModelCapabilities, SlotType};
pub mod model_discovery;
