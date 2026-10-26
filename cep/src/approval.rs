//! Approvals: the owner's yes for one exact call. An approval is bound to the
//! principal, the method and a hash of the params, is used once, and expires.

use crate::error::CepError;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

/// At most this many approvals are kept; used and expired ones go first
const MAX_KEPT: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Approved,
    Rejected,
    Used,
}

#[derive(Debug, Clone, Serialize)]
pub struct Approval {
    pub id: String,
    pub principal: String,
    pub method: String,
    /// What the owner is shown: a path, a command, a tool
    pub target: String,
    #[serde(skip)]
    pub digest: [u8; 32],
    pub created_at: String,
    pub expires_at: String,
    pub status: Status,
    #[serde(skip)]
    expires_ms: u64,
}

/// The owner's answer, for a caller that waits (the chat agent loop)
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub approved: bool,
    pub feedback: Option<String>,
}

pub struct ApprovalStore {
    ttl: Duration,
    items: Mutex<HashMap<String, Approval>>,
    waiters: Mutex<HashMap<String, oneshot::Sender<Decision>>>,
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub(crate) fn rfc3339(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// SHA-256 of the params with object keys sorted, so key order never changes the hash
pub fn params_digest(params: &Value) -> [u8; 32] {
    fn canonical(value: &Value, out: &mut String) {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                out.push('{');
                for (i, key) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&Value::String(key.clone()).to_string());
                    out.push(':');
                    canonical(&map[key], out);
                }
                out.push('}');
            }
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    canonical(item, out);
                }
                out.push(']');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut text = String::new();
    canonical(params, &mut text);
    Sha256::digest(text.as_bytes()).into()
}

impl ApprovalStore {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, items: Mutex::new(HashMap::new()), waiters: Mutex::new(HashMap::new()) }
    }

    fn items(&self) -> std::sync::MutexGuard<'_, HashMap<String, Approval>> {
        self.items.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn prune(items: &mut HashMap<String, Approval>, now: u64) {
        items.retain(|_, a| a.expires_ms > now && a.status != Status::Used && a.status != Status::Rejected);
        while items.len() >= MAX_KEPT {
            let Some(oldest) = items.values().min_by_key(|a| a.expires_ms).map(|a| a.id.clone()) else { break };
            items.remove(&oldest);
        }
    }

    /// A new pending approval
    pub fn request(&self, principal: &str, method: &str, digest: [u8; 32], target: &str) -> Approval {
        let now = now_ms();
        let expires_ms = now + self.ttl.as_millis() as u64;
        let approval = Approval {
            id: format!("ap_{}", uuid::Uuid::new_v4().simple()),
            principal: principal.into(),
            method: method.into(),
            target: target.into(),
            digest,
            created_at: rfc3339(now),
            expires_at: rfc3339(expires_ms),
            status: Status::Pending,
            expires_ms,
        };
        let mut items = self.items();
        Self::prune(&mut items, now);
        items.insert(approval.id.clone(), approval.clone());
        approval
    }

    /// True once: when `id` is approved, not expired, and was made for this principal, method and
    /// params. It is then used up.
    pub fn consume(&self, id: &str, principal: &str, method: &str, digest: [u8; 32]) -> bool {
        let mut items = self.items();
        let Some(approval) = items.get_mut(id) else { return false };
        let valid = approval.status == Status::Approved
            && approval.expires_ms > now_ms()
            && approval.principal == principal
            && approval.method == method
            && approval.digest == digest;
        if valid {
            approval.status = Status::Used;
        }
        valid
    }

    /// The owner's answer to a pending approval
    pub fn decide(&self, id: &str, approved: bool, feedback: Option<String>) -> Result<Approval, CepError> {
        let decided = {
            let mut items = self.items();
            let approval = items
                .get_mut(id)
                .filter(|a| a.expires_ms > now_ms())
                .ok_or_else(|| CepError::not_found("approval_not_found", format!("No pending approval {id}")))?;
            if approval.status != Status::Pending {
                return Err(CepError::conflict("approval_decided", format!("Approval {id} is already {:?}", approval.status)));
            }
            approval.status = if approved { Status::Approved } else { Status::Rejected };
            approval.clone()
        };
        if let Some(waiter) = self.waiters.lock().unwrap_or_else(|e| e.into_inner()).remove(id) {
            let _ = waiter.send(Decision { approved, feedback });
        }
        Ok(decided)
    }

    /// Waits for the owner's answer to `id` (the receiver ends if the approval is dropped)
    pub fn wait(&self, id: &str) -> oneshot::Receiver<Decision> {
        let (tx, rx) = oneshot::channel();
        self.waiters.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), tx);
        rx
    }

    /// Stops waiting (the caller gave up), and forgets a still-pending approval
    pub fn forget(&self, id: &str) {
        self.waiters.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        let mut items = self.items();
        if items.get(id).is_some_and(|a| a.status == Status::Pending) {
            items.remove(id);
        }
    }

    pub fn pending(&self) -> Vec<Approval> {
        let now = now_ms();
        let mut items = self.items();
        Self::prune(&mut items, now);
        let mut pending: Vec<Approval> = items.values().filter(|a| a.status == Status::Pending).cloned().collect();
        pending.sort_by_key(|a| a.expires_ms);
        pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_an_approval_fits_one_call_once() {
        let store = ApprovalStore::new(Duration::from_secs(300));
        let digest = params_digest(&json!({"path": "D:/x", "text": "hi"}));
        let approval = store.request("extension:e", "fs.write", digest, "D:/x");
        // Not approved yet
        assert!(!store.consume(&approval.id, "extension:e", "fs.write", digest));
        store.decide(&approval.id, true, None).unwrap();
        // Other principal, method or params: no
        assert!(!store.consume(&approval.id, "token:t", "fs.write", digest));
        assert!(!store.consume(&approval.id, "extension:e", "fs.delete", digest));
        assert!(!store.consume(&approval.id, "extension:e", "fs.write", params_digest(&json!({"path": "D:/y"}))));
        assert!(store.consume(&approval.id, "extension:e", "fs.write", digest));
        // Used up
        assert!(!store.consume(&approval.id, "extension:e", "fs.write", digest));
        assert!(store.decide(&approval.id, true, None).is_err());
    }

    #[test]
    fn test_expiry_rejection_and_key_order() {
        let store = ApprovalStore::new(Duration::from_millis(0));
        let digest = params_digest(&json!({"a": 1, "b": [1, {"c": 2, "d": 3}]}));
        assert_eq!(digest, params_digest(&json!({"b": [1, {"d": 3, "c": 2}], "a": 1})));
        let expired = store.request("p", "m", digest, "t");
        assert!(store.decide(&expired.id, true, None).is_err());
        let store = ApprovalStore::new(Duration::from_secs(60));
        let rejected = store.request("p", "m", digest, "t");
        assert_eq!(store.pending().len(), 1);
        store.decide(&rejected.id, false, None).unwrap();
        assert!(!store.consume(&rejected.id, "p", "m", digest));
        assert!(store.pending().is_empty());
    }

    #[tokio::test]
    async fn test_a_waiter_gets_the_decision() {
        let store = ApprovalStore::new(Duration::from_secs(60));
        let approval = store.request("agent", "tools.call", [0; 32], "bash");
        let rx = store.wait(&approval.id);
        store.decide(&approval.id, true, Some("ok".into())).unwrap();
        assert_eq!(rx.await.unwrap(), Decision { approved: true, feedback: Some("ok".into()) });
    }
}
