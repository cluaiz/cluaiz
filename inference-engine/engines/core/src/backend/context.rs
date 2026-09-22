use crate::metadata::dna::StructuralDNA;

use crate::hardware::HardwareGovernor;
use crate::prompting::templater::TemplateManager;


/// EngineContext: The unified state object bridging user intent, system truth, and active models.
#[derive(Clone)]
pub struct EngineContext {
    pub dna: StructuralDNA,
    pub governor: HardwareGovernor,
    pub templater: TemplateManager,
}


impl EngineContext {
    /// Initialize a high-performance execution context
    pub fn boot(dna: StructuralDNA, templater: TemplateManager) -> Self {
        let governor = HardwareGovernor::start();
        
        Self {
            dna,
            governor,
            templater,
        }
    }
}
