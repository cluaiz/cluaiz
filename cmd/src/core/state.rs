//! State of the model list (`/models` in the chat: browse, download, delete).

pub struct AppState {
    pub hardware: cluaiz_hw::SystemInfo,
    pub ram_gb: f64,
    /// Model list rows (filled when the list is opened)
    pub sorted_models: Vec<cluaiz_models::ModelRecommendation>,
}

impl AppState {
    pub fn new() -> Self {
        let hardware = cluaiz_hw::HardwareProber::current();
        let ram_gb = hardware.memory.total_gb;
        Self { hardware, ram_gb, sorted_models: Vec::new() }
    }

    /// Applies download events from the model list (refreshes the rows when a download finished).
    pub fn handle_events(&mut self, rx: &mut tokio::sync::mpsc::UnboundedReceiver<cluaiz_models::DownloadEvent>) {
        while let Ok(event) = rx.try_recv() {
            if let cluaiz_models::DownloadEvent::Complete(_) = event {
                self.sorted_models = cluaiz_models::CoreRoster::get_recommendations(&self.hardware, self.ram_gb);
            }
        }
    }
}
