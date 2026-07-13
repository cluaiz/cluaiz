// cluaiz-engine: Core Foundry - Skill Scanner
// Scans the cluaiz skills directory for package manifests.

use std::path::{Path, PathBuf};
use std::fs;

pub struct SkillScanner {
    base_path: PathBuf,
}

impl SkillScanner {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            base_path: path.as_ref().to_path_buf(),
        }
    }

    /// Scans recursively for manifest.json files and returns their paths.
    pub fn scan_manifests(&self) -> Vec<PathBuf> {
        let mut manifests = Vec::new();
        if !self.base_path.exists() {
            cluaiz_shared::dev_info!("[cluaiz] [WARN] Skills directory not found: {:?}", self.base_path);
            return manifests;
        }

        self.walk_dir(&self.base_path, &mut manifests);
        cluaiz_shared::dev_info!("[cluaiz] Found {} skill manifests.", manifests.len());
        manifests
    }

    fn walk_dir(&self, dir: &Path, manifests: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip .system and hidden folders
                    if !path.file_name().and_then(|n| n.to_str()).map(|s| s.starts_with('.')).unwrap_or(false) {
                        self.walk_dir(&path, manifests);
                    }
                } else if path.file_name().map(|n| {
                    let n = n.to_string_lossy();
                    n == "manifest.json" || n == "SKILL.md" || 
                    (n.starts_with("manifest-") && (n.ends_with(".yaml") || n.ends_with(".yml") || n.ends_with(".json")))
                }).unwrap_or(false) {
                    manifests.push(path);
                }
            }
        }
    }
}
