//! Shared asset requirements for explicitly supported export manifests.
//! This is metadata evidence, not graph compatibility or execution proof.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, path::Path};

#[derive(Debug, Deserialize)]
pub struct AssetContract {
    pub manifest: String,
    pub max_manifest_bytes: u64,
    selector: BTreeMap<String, Value>,
    requires: BTreeMap<String, Value>,
    precision: String,
    precisions: String,
    primary_role: String,
    graph_roles: Vec<String>,
    auxiliary_roots: Vec<String>,
    assets: BTreeMap<String, AssetRule>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AssetRule {
    Template { template: String },
    Map { map: String, key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageAssets {
    pub manifest: String,
    pub precision: String,
    pub primary: String,
    pub assets: BTreeMap<String, String>,
    #[serde(default)]
    pub graphs: Vec<String>,
    #[serde(default)]
    pub auxiliary_roots: Vec<String>,
}

impl AssetContract {
    pub fn builtins() -> Result<Vec<Self>> {
        Ok(serde_json::from_str(include_str!(
            "model_assets/contracts.json"
        ))?)
    }

    /// Unknown exports return None, recognized incompatible exports return errors.
    /// Resolve the declared default only, matching current native runtime loading.
    pub fn resolve(&self, value: &Value) -> Result<Option<PackageAssets>> {
        if !self
            .selector
            .iter()
            .all(|(key, expected)| value.pointer(key) == Some(expected))
        {
            return Ok(None);
        }
        for (key, expected) in &self.requires {
            if value.pointer(key) != Some(expected) {
                bail!("Unsupported package export: {key} must be {expected}");
            }
        }
        let text = |pointer: &str| -> Result<&str> {
            value
                .pointer(pointer)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .with_context(|| format!("Package manifest requires string {pointer}"))
        };
        let precision = text(&self.precision)?;
        if !value
            .pointer(&self.precisions)
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(precision)))
        {
            bail!("Package default precision is not declared available");
        }
        let mut assets = BTreeMap::new();
        for (role, rule) in &self.assets {
            let path = match rule {
                AssetRule::Template { template } => template.replace("{precision}", precision),
                AssetRule::Map { map, key } => value
                    .pointer(map)
                    .and_then(|v| v.get(text(key).ok()?))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .with_context(|| format!("Package manifest has no asset for {role}"))?
                    .to_owned(),
            };
            safe_asset_path(&path)?;
            assets.insert(role.clone(), path);
        }
        let primary = assets
            .get(&self.primary_role)
            .context("Package contract lacks primary role")?
            .clone();
        let graphs = self
            .graph_roles
            .iter()
            .map(|role| {
                assets
                    .get(role)
                    .cloned()
                    .with_context(|| format!("Package graph role missing: {role}"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(PackageAssets {
            manifest: self.manifest.clone(),
            precision: precision.to_owned(),
            primary,
            assets,
            graphs,
            auxiliary_roots: self.auxiliary_roots.clone(),
        }))
    }
}

/// Shared lexical gate; filesystem consumers must also check canonical containment.
pub fn safe_asset_path(value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains(['\\', ':'])
        || !Path::new(value)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        bail!("Invalid repository-relative asset path: {value}");
    }
    Ok(())
}

impl PackageAssets {
    pub fn required_files(&self) -> Vec<String> {
        let mut paths: Vec<_> = self.assets.values().cloned().collect();
        paths.push(self.manifest.clone());
        paths.sort();
        paths.dedup();
        paths
    }

    pub fn file(&self, role: &str) -> Result<&str> {
        self.assets
            .get(role)
            .map(String::as_str)
            .with_context(|| format!("Package role missing: {role}"))
    }

    /// Bounded metadata only; this never loads weights or opens a runtime session.
    pub fn from_directory(dir: &Path) -> Result<Option<Self>> {
        for contract in AssetContract::builtins()? {
            let path = dir.join(&contract.manifest);
            if !path.exists() {
                continue;
            }
            let canonical = path.canonicalize()?;
            if !canonical.starts_with(dir.canonicalize()?) {
                bail!("Package manifest escapes its directory");
            }
            let mut bytes = Vec::new();
            std::fs::File::open(canonical)?
                .take(contract.max_manifest_bytes + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > contract.max_manifest_bytes {
                bail!("Package manifest exceeds metadata limit");
            }
            let value = serde_json::from_slice(&bytes).context("Invalid package manifest JSON")?;
            if let Some(package) = contract.resolve(&value)? {
                return Ok(Some(package));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "model_assets/tests.rs"]
mod tests;
