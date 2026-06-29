use crate::SetupCommand;

pub async fn execute(command: SetupCommand) -> color_eyre::Result<()> {
    match command {
        SetupCommand::Profile => {
            println!("⚙️ [cluaiz Setup] Initiating Node Purpose Vectorization...");
            println!("⚙️ [cluaiz Setup] Generating semantic vectors for SKILL_NODE_ROOT identity...");
            
            let prompt_vector = engines::memory::embedding_generator::EmbeddingGenerator::generate_vector("SKILL_NODE_ROOT");
            
            let storage_bridge = engines::memory::storage_bridge::load_storage_bridge();
            let _ = storage_bridge.save_context("SKILL_NODE_ROOT_IDENTITY", "System Profile Node", &prompt_vector)
                .map_err(|e| color_eyre::eyre::eyre!("Failed to save vector to brain plugin: {}", e))?;
                
            println!("✅ [cluaiz Setup] Purpose Vectorization saved to Local Brain.");
        }
    }
    Ok(())
}
