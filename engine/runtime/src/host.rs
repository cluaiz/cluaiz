//! 🏛️ Single Engine Host (`engine/runtime`)
//! Central model host replacing dispatcher, CoreRouter, and EngineRunner.

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use cluaiz_core::ActiveUserSettings;
use cluaiz_hw::SystemInfo;
use cluaiz_models::ModelGeometry;
use cluaiz_plan::{ExecutionPlan, Planner};

pub enum HostResponse {
    TokenStream(mpsc::Receiver<String>),
    Completed(String),
    Error(String),
}

pub struct EngineHost {
    pub current_plan: Arc<Mutex<Option<ExecutionPlan>>>,
}

impl EngineHost {
    pub fn new() -> Self {
        Self {
            current_plan: Arc::new(Mutex::new(None)),
        }
    }

    /// Plans and prepares model execution
    pub async fn prepare_model(
        &self,
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> ExecutionPlan {
        let plan = Planner::create_plan(model_id, geometry, hardware, settings);
        let mut guard = self.current_plan.lock().await;
        *guard = Some(plan.clone());
        plan
    }

    /// Dispatches prompt generation stream
    pub async fn dispatch_stream(&self, prompt: &str) -> HostResponse {
        let (tx, rx) = mpsc::channel::<String>(100);
        let prompt_clone = prompt.to_string();

        tokio::spawn(async move {
            // Emits tokens sequentially
            let _ = tx.send(format!("Ready: {}", prompt_clone)).await;
        });

        HostResponse::TokenStream(rx)
    }
}
