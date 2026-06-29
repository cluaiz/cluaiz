//! Master Registry Index — Tier 1 of the Two-Tier Registry Architecture
//!
//! Reads the global `registry.yaml` file once at engine cold boot.
//! Returns a `RegistryIndex` that classifies all installed components into
//! EAGER (load at startup), LAZY (load on activation event), or MANUAL (never auto-load).
//!
//! **Path injection policy:** This module NEVER resolves `~/.cluaiz` internally.
//! The path to `registry.yaml` is always injected by the caller (e.g., `HardwareGovernor`).
//! This enforces CERD LAW 6: No Hardcoded Knowledge.
//!
//! ## registry.yaml format
//! ```yaml
//! version: "1.0.0"
//! schema: "cluaiz-registry-v1"
//!
//! extensions:
//!   cluaiz-db:
//!     id: "ext_core_db_001"
//!     domain: "core/cluaiz-db"
//!     load_strategy: "EAGER"
//!     activation_events: ["on_startup"]
//!     enabled: true
//!     binary_hash: "sha256:a4fbc89e..."
//!
//!   web-scraper:
//!     id: "plugin_tool_scrape_002"
//!     domain: "tools/web-scraper"
//!     load_strategy: "LAZY"
//!     activation_events: ["on_command:use plugin::web-scraper", "on_cel_keyword:scrape"]
//!     enabled: true
//!     binary_hash: "sha256:f92bc31a..."
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use sha2::{Sha256, Digest};

/// The loading strategy for a registry entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LoadStrategy {
    /// Load binary into RAM immediately at engine startup.
    Eager,
    /// Register activation events. Zero RAM allocated until triggered.
    Lazy,
    /// Never auto-load. Only loads via explicit `cluaiz load <name>` command.
    Manual,
}

/// A single entry in the master registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Unique identifier for this component.
    pub id: String,
    /// Relative path under the cluaiz home directory (e.g., `core/cluaiz-db`).
    /// Resolved to an absolute path by the caller using their own path resolution.
    pub domain: String,
    /// When and how to load this component.
    pub load_strategy: LoadStrategy,
    /// Events that trigger loading for LAZY components.
    /// Ignored for EAGER and MANUAL components.
    #[serde(default)]
    pub activation_events: Vec<String>,
    /// Whether this component is active. `false` = completely skip, even if triggered.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// SHA-256 hash of the binary for integrity verification before loading.
    /// Format: `"sha256:<hex_digest>"`. `None` = skip hash verification (not recommended).
    pub binary_hash: Option<String>,
    /// Component name (derived from the map key during deserialization).
    #[serde(skip)]
    pub name: String,
}

fn default_true() -> bool { true }

/// The raw registry file structure as deserialized from `registry.yaml`.
#[derive(Debug, Deserialize)]
struct RawRegistry {
    #[allow(dead_code)]
    version: Option<String>,
    #[allow(dead_code)]
    schema: Option<String>,
    #[serde(default)]
    extensions: HashMap<String, RegistryEntry>,
    #[serde(default)]
    plugins: HashMap<String, RegistryEntry>,
    #[serde(default)]
    mcp: HashMap<String, RegistryEntry>,
}

/// The classified result of parsing `registry.yaml`.
/// Used by the engine boot sequence and `ActivationEventBus`.
#[derive(Debug)]
pub struct RegistryIndex {
    /// Components to load immediately at engine startup.
    pub eager: Vec<RegistryEntry>,
    /// Components to register activation events for — zero RAM until triggered.
    pub lazy: Vec<RegistryEntry>,
    /// Components that will never auto-load.
    pub manual: Vec<RegistryEntry>,
    /// Components that are disabled — completely ignored.
    pub disabled: Vec<RegistryEntry>,
}

/// Reads and classifies the master `registry.yaml` file.
pub struct MasterRegistry;

impl MasterRegistry {
    /// Parses `registry.yaml` from the given path and returns a classified `RegistryIndex`.
    ///
    /// **Path is always injected by the caller.** This function never resolves
    /// `~/.cluaiz` or any other home-directory path internally.
    ///
    /// Only `enabled: true` entries are classified into eager/lazy/manual.
    /// Disabled entries are collected separately for diagnostic purposes.
    pub fn load_from_path(registry_yaml_path: &Path) -> Result<RegistryIndex, String> {
        let content = std::fs::read_to_string(registry_yaml_path).map_err(|e| {
            format!(
                "registry.yaml not found at {:?}: {}. \
                 Run `cluaiz init` to create it.",
                registry_yaml_path, e
            )
        })?;

        let mut raw: RawRegistry = serde_yaml::from_str(&content).map_err(|e| {
            format!("Failed to parse registry.yaml at {:?}: {}", registry_yaml_path, e)
        })?;

        // Merge all component maps and inject the map key as the `name` field
        let mut all_entries: Vec<RegistryEntry> = Vec::new();

        for (name, mut entry) in raw.extensions.drain() {
            entry.name = name;
            all_entries.push(entry);
        }
        for (name, mut entry) in raw.plugins.drain() {
            entry.name = name;
            all_entries.push(entry);
        }
        for (name, mut entry) in raw.mcp.drain() {
            entry.name = name;
            all_entries.push(entry);
        }

        // Classify entries
        let mut index = RegistryIndex {
            eager: Vec::new(),
            lazy: Vec::new(),
            manual: Vec::new(),
            disabled: Vec::new(),
        };

        for entry in all_entries {
            if !entry.enabled {
                tracing::debug!("Registry: '{}' is disabled — skipping.", entry.name);
                index.disabled.push(entry);
                continue;
            }

            match entry.load_strategy {
                LoadStrategy::Eager => {
                    tracing::debug!("Registry: '{}' classified as EAGER.", entry.name);
                    index.eager.push(entry);
                }
                LoadStrategy::Lazy => {
                    tracing::debug!(
                        "Registry: '{}' classified as LAZY with {} activation events.",
                        entry.name,
                        entry.activation_events.len()
                    );
                    index.lazy.push(entry);
                }
                LoadStrategy::Manual => {
                    tracing::debug!("Registry: '{}' classified as MANUAL.", entry.name);
                    index.manual.push(entry);
                }
            }
        }

        tracing::info!(
            "Registry loaded: {} EAGER, {} LAZY, {} MANUAL, {} disabled.",
            index.eager.len(),
            index.lazy.len(),
            index.manual.len(),
            index.disabled.len()
        );

        Ok(index)
    }

    /// Verifies the SHA-256 hash of a binary file against the expected hash from the registry.
    ///
    /// Uses streaming I/O — O(1) memory regardless of file size (H-3 security fix).
    /// Expected format: `"sha256:<lowercase_hex_digest>"`
    ///
    /// Returns `Ok(())` if the hash matches, `Err(...)` if it doesn't.
    /// Callers should refuse to load any binary that fails this check.
    pub fn verify_hash(binary_path: &Path, expected_hash: &str) -> Result<(), String> {
        use std::io;

        let expected_hex = expected_hash
            .strip_prefix("sha256:")
            .ok_or_else(|| format!("Invalid hash format '{}'. Must start with 'sha256:'.", expected_hash))?;

        let mut file = std::fs::File::open(binary_path).map_err(|e| {
            format!("Cannot open binary for hash verification at {:?}: {}", binary_path, e)
        })?;

        let mut hasher = Sha256::new();

        // Streaming hash — copies 8KB at a time, never loads the full binary into RAM
        io::copy(&mut file, &mut hasher).map_err(|e| {
            format!("Failed to read binary for hashing at {:?}: {}", binary_path, e)
        })?;

        let actual_hex = format!("{:x}", hasher.finalize());

        if actual_hex == expected_hex {
            tracing::debug!("Hash verification passed for {:?}.", binary_path);
            Ok(())
        } else {
            Err(format!(
                "Hash verification FAILED for {:?}. \
                 Expected sha256:{} but got sha256:{}. \
                 The binary may be corrupted or tampered with.",
                binary_path, expected_hex, actual_hex
            ))
        }
    }
}
