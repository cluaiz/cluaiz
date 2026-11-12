//! `engine` triggers: Engine events (docs/automation/events.md#engine-events) start the flows
//! waiting for them. A flow's own runs do not start it again through `flow.finished` or
//! `flow.failed`.

use crate::service::Service;
use cluaiz_automation::store::Scope;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

/// Subscribes at once (so `engine.started` is not missed), then loads the flows and passes each
/// event to the flows that wait for it
pub fn listen(service: Arc<Service>) {
    let mut events = cluaiz_cep::events::subscribe();
    tokio::spawn(async move {
        super::sync_all(&service, true).await;
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!("Automation missed {missed} Engine events (too many at once)");
                    continue;
                }
                Err(RecvError::Closed) => return,
            };
            if event.name == crate::reach::EMITTED {
                let name = event.data["name"].as_str().unwrap_or_default();
                let from = &event.data["from"];
                let from_scope = from["scope"].as_str().and_then(Scope::parse).unwrap_or(Scope::User);
                for (scope, flow, id) in service.triggers.listening_for_emit(name, from_scope, from["flow"].as_str().unwrap_or_default()) {
                    let data = json!({ "name": name, "data": event.data["data"], "from": from, "hops": event.data["hops"], "at": event.at });
                    let service = service.clone();
                    tokio::spawn(async move { super::fire(&service, scope, &flow, &id, data).await });
                }
                continue;
            }
            for (scope, name, id) in service.triggers.listening_for(&event.name) {
                let own = event.data.get("flow").and_then(|f| f.as_str()) == Some(name.as_str()) && event.data.get("scope").and_then(|s| s.as_str()) == Some(scope.as_str());
                if own {
                    continue;
                }
                let data = json!({ "event": event.name, "data": event.data, "at": event.at });
                let service = service.clone();
                tokio::spawn(async move { super::fire(&service, scope, &name, &id, data).await });
            }
        }
    });
}
