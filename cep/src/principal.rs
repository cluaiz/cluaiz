//! Who is calling, and what they may do. A principal always comes from the
//! credential the transport checked, never from a field in the request.

use crate::error::CepError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The one scope a method needs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// No side effects
    Read,
    /// Changes Engine state, or files inside the roots
    Write,
    /// Runs code: tools, plugins, MCP servers
    Exec,
    /// Tokens, approvals, security settings
    Admin,
}

impl Scope {
    pub const ALL: [Scope; 4] = [Scope::Read, Scope::Write, Scope::Exec, Scope::Admin];

    pub fn name(self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::Write => "write",
            Scope::Exec => "exec",
            Scope::Admin => "admin",
        }
    }
}

/// What happens when a principal goes past its roots
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnDenied {
    /// The call fails with `forbidden`
    Error,
    /// The owner is asked (an approval)
    Ask,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Principal {
    /// "owner", "token:<name>", "extension:<id>", "mcp"
    pub id: String,
    pub scopes: Vec<Scope>,
    /// Folders `fs.*` may touch; `None` = anywhere (the owner)
    pub roots: Option<Vec<PathBuf>>,
    pub on_denied: OnDenied,
    /// Only the owner approves, and only the owner opens UI on the host
    pub owner: bool,
}

impl Principal {
    /// The person at the machine: the session token, the native pipe, the in-process library, or
    /// any caller while API authentication is off (the owner's setting)
    pub fn owner() -> Self {
        Self { id: "owner".into(), scopes: Scope::ALL.to_vec(), roots: None, on_denied: OnDenied::Error, owner: true }
    }

    pub fn has(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }

    /// The scope check every call goes through before its method runs
    pub fn allow(&self, scope: Scope, owner_only: bool) -> Result<(), CepError> {
        if owner_only && !self.owner {
            return Err(CepError::forbidden("owner"));
        }
        if !self.has(scope) {
            return Err(CepError::forbidden(scope.name()));
        }
        Ok(())
    }

    /// Whether a resolved (canonical) path is inside the roots
    pub fn within_roots(&self, path: &Path) -> bool {
        match &self.roots {
            None => true,
            Some(roots) => roots.iter().any(|root| path.starts_with(root)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FORBIDDEN;

    fn token(scopes: &[Scope], roots: &[&str]) -> Principal {
        Principal {
            id: "token:t".into(),
            scopes: scopes.to_vec(),
            roots: Some(roots.iter().map(PathBuf::from).collect()),
            on_denied: OnDenied::Error,
            owner: false,
        }
    }

    #[test]
    fn test_scopes_roots_and_owner_only() {
        let owner = Principal::owner();
        let reader = token(&[Scope::Read], &["/work"]);
        for scope in Scope::ALL {
            assert!(owner.allow(scope, true).is_ok());
            assert_eq!(reader.allow(scope, false).is_ok(), scope == Scope::Read);
        }
        let denied = reader.allow(Scope::Write, false).unwrap_err();
        assert_eq!((denied.code, denied.data.unwrap()["needs"].as_str().unwrap()), (FORBIDDEN, "write"));
        assert_eq!(reader.allow(Scope::Read, true).unwrap_err().data.unwrap()["needs"], "owner");
        assert!(reader.within_roots(Path::new("/work/a.txt")));
        assert!(!reader.within_roots(Path::new("/workshop/a.txt")));
        assert!(!reader.within_roots(Path::new("/other")));
        assert!(owner.within_roots(Path::new("/anything")));
    }
}
