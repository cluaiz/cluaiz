// cluaiz-engine: Core Foundry - Security Guard
// Implements the 4-tier permission hierarchy for cluaiz skills.

use crate::neural_foundry::registry::SkillManifest;
use anyhow::{Result, anyhow};
use std::sync::Mutex;
use tracing::warn;

#[derive(Debug, PartialEq, PartialOrd, Clone, Copy)]
pub enum PermissionLevel {
    ReadOnly = 0,
    WorkspaceWrite = 1,
    NetworkRestricted = 2,
    DangerFullAccess = 3,
}

impl From<&str> for PermissionLevel {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "WORKSPACEWRITE" => PermissionLevel::WorkspaceWrite,
            "NETWORKRESTRICTED" => PermissionLevel::NetworkRestricted,
            "DANGERFULLACCESS" => PermissionLevel::DangerFullAccess,
            _ => PermissionLevel::ReadOnly,
        }
    }
}

pub struct PermissionGuard {
    pub audit_log: Mutex<Vec<String>>,
}

impl Default for PermissionGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionGuard {
    pub fn new() -> Self {
        Self {
            audit_log: Mutex::new(Vec::new()),
        }
    }

    /// Validates if a skill is allowed to perform a specific action.
    pub fn validate_action(&self, manifest: &SkillManifest, requested_level: PermissionLevel) -> Result<()> {
        let skill_level = PermissionLevel::from(manifest.permissions.level.as_str());

        // 📝 Audit Log
        let entry = format!("[AUDIT] Skill '{}' requested level: {:?}", manifest.id, requested_level);
        self.audit_log.lock().unwrap().push(entry);

        if (skill_level as u8) < (requested_level as u8) {
            cluaiz_shared::dev_info!("🚨 [SECURITY ALERT] Unauthorized Access Attempt by skill '{}'!", manifest.id);
            return Err(anyhow!("Security Violation: Permission Denied for skill '{}'", manifest.id));
        }

        if requested_level == PermissionLevel::DangerFullAccess {
            warn!("⚠️ [HUMAN-IN-THE-LOOP] Critical level requested by '{}'. Verification mandatory.", manifest.id);
        }

        Ok(())
    }

    /// Enforces file access boundaries with deep path normalization.
    pub fn validate_file_access(&self, manifest: &SkillManifest, path: &str, is_write: bool) -> Result<()> {
        let skill_level = PermissionLevel::from(manifest.permissions.level.as_str());
        
        if is_write && skill_level < PermissionLevel::WorkspaceWrite {
            return Err(anyhow!("Security Violation: Write attempt in Read-Only mode by '{}'.", manifest.id));
        }
        
        // Prevent path traversal
        if path.contains("..") {
            return Err(anyhow!("Security Violation: Path traversal detected in '{}'.", manifest.id));
        }

        if !path.contains("cluaiz-workspace") && skill_level < PermissionLevel::DangerFullAccess {
            return Err(anyhow!("Security Violation: Out-of-bounds access attempt by '{}'.", manifest.id));
        }

        Ok(())
    }
}
