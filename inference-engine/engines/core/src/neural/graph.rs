//! 🧠 Tier 12: Neural Graph Chronicle
//! Responsible for logging all cognitive activity into the Sovereign Graph (thing.ai.nurale.md).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use chrono::Local;

pub struct NeuralGraph;

impl NeuralGraph {
    /// 🖋️ Records a neural pulse (activity) into the sovereign graph.
    pub fn chronicle_pulse(activity: &str, entity: &str, metadata: &str) -> anyhow::Result<()> {
        let path = Self::resolve_graph_path();
        
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        // 📊 Sovereign Markdown Format for Neural Activity
        let entry = format!(
            "### [ {timestamp} ] 🧠 {activity}\n- **Entity**: `{entity}`\n- **Status**: ✅ Synchronized\n- **Metadata**: {metadata}\n\n---\n\n"
        );

        file.write_all(entry.as_bytes())?;
        Ok(())
    }

    /// 🗺️ Resolves the path to the Neural Graph in the workspace root.
    fn resolve_graph_path() -> PathBuf {
        let mut current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        
        // Traverse up to find the true workspace root
        for _ in 0..10 {
            // Check if we are in the workspace root by looking for 'cluaiz-engine' or 'thing.ai.nurale.md'
            if current.join("cluaiz-engine").exists() || current.join("thing.ai.nurale.md").exists() {
                return current.join("thing.ai.nurale.md");
            }
            if let Some(parent) = current.parent() {
                current = parent.to_path_buf();
            } else {
                break;
            }
        }
        
        PathBuf::from("thing.ai.nurale.md") // Fallback
    }
}
