pub mod parser;
pub mod execution;
pub mod ffi;
pub mod vram;

// Parser public API
pub use parser::ast::{CelOp, CelValue, CelAst};
pub use parser::planner::ExecutionPlan;
pub use parser::metadata_parser::{EngineRules, FfiBindings, IntegrationMetadata, Integration};
pub use parser::parse_cel;

// Execution public API
pub use execution::wasm_sandbox::WasmExecutor;
pub use execution::registry::CluaizxtensionRegistry;
pub use execution::registry_index::{MasterRegistry, RegistryIndex, RegistryEntry, LoadStrategy};
pub use execution::activation_bus::ActivationEventBus;
pub use execution::Cluaizxecutor;
