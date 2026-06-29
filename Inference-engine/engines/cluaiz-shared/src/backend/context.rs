use crate::metadata::dna::StructuralDNA;

use crate::hardware::HardwareGovernor;
use crate::prompting::templater::TemplateManager;


/// cluaizContext: The unified state object bridging user intent, system truth, and active models.
#[derive(Clone)]
pub struct cluaizContext {
    pub dna: StructuralDNA,
    pub governor: HardwareGovernor,
    pub templater: TemplateManager,
}

impl cluaizContext {
    /// Initialize a high-performance sovereign context
    pub fn boot(dna: StructuralDNA, templater: TemplateManager) -> Self {
        let governor = HardwareGovernor::start();
        
        Self {
            dna,
            governor,
            templater,
        }
    }
}
