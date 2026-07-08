use crate::app_enums::Tab;
use ratatui::widgets::{TableState, ListState};
use std::sync::Arc;

// ── Re-export shared types for CLI use ──


#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OsState {
    MainMenu,
    Dashboard,
}


#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuApp {
    None,
    Roster,
    Settings,
    Help,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RosterView {
    List,
    Details,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SettingsTab {
    Main,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AppMode {
    Normal,
    CommandPalette, // Triggered by /
    ContextPalette, // Triggered by @
}

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct CoreEngine {
    pub router: Arc<Mutex<engines::CoreRouter>>,
    pub is_loaded: Arc<AtomicBool>,
    pub loading_error: Arc<Mutex<Option<String>>>,
}

impl CoreEngine {
    pub fn new() -> Self {
        Self { 
            router: Arc::new(tokio::sync::Mutex::new(engines::CoreRouter::new())),
            is_loaded: Arc::new(AtomicBool::new(false)),
            loading_error: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn load_model(&self, path: std::path::PathBuf) -> Result<(), String> {
        let backend = engines::BackendType::RuntimeB;

        // 🔄 Reset Linker State
        self.is_loaded.store(false, Ordering::SeqCst);
        {
            let mut err_lock = self.loading_error.lock().await;
            *err_lock = None;
        }
        
        match engines::CoreRouter::load_model(path, backend).await {
            Ok(router) => {
                let mut lock = self.router.lock().await;
                *lock = router;
                self.is_loaded.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                let mut err_lock = self.loading_error.lock().await;
                *err_lock = Some(e.clone());
                Err(e)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum ActivityBlock {
    // ── Frozen History Records (permanent, compact) ──
    SystemLog(String),
    MenuOpened(String),
    ModelSelected { name: String, ctx: String, status: String },
    DownloadStarted(String),
    DownloadComplete(String),
    DownloadFailed(String, String),
    ModelMounted(String),
    Chat(String, String),

    // ── Live Interactive Blocks (only one active at a time) ──
    ActiveRoster,
    ActiveDetails,
}

impl ActivityBlock {
    pub fn is_live(&self) -> bool {
        matches!(self, ActivityBlock::ActiveRoster | ActivityBlock::ActiveDetails)
    }

    pub fn get_frozen_height(&self, width: u16) -> u16 {
        match self {
            ActivityBlock::SystemLog(_) => 1,
            ActivityBlock::MenuOpened(_) => 1,
            ActivityBlock::ModelSelected { .. } => 1,
            ActivityBlock::DownloadStarted(_) => 1,
            ActivityBlock::DownloadComplete(_) => 1,
            ActivityBlock::DownloadFailed(_, _) => 1,
            ActivityBlock::ModelMounted(_) => 1,
            ActivityBlock::Chat(sender, msg) => {
                let prefix_len = if sender == "USER" { 7 } else { 4 }; // "👤 USER " vs "🤖  "
                let total_len = prefix_len + msg.len();
                let w = width.saturating_sub(2); // Margin
                if w == 0 { return 1; }
                ((total_len as f32 / w as f32).ceil()) as u16
            }
            // Live blocks have 0 frozen height — they use active_block_h in layout
            ActivityBlock::ActiveRoster | ActivityBlock::ActiveDetails => 0,
        }
    }
}

pub struct AppState {
    pub os_state: OsState,
    pub username: String,
    pub frame_counter: u64,
    pub _active_tab: Tab,
    pub active_app: MenuApp,
    pub _menu_app: MenuApp,
    pub hardware: ::cluaiz_shared::hardware::schema::profiles::cluaizProfile,
    pub ram_gb: f64,
    pub sorted_models: Vec<engines::ModelRecommendation>,
    pub roster_state: TableState,
    pub menu_state: ListState,
    pub roster_h_scroll: u16,
    pub details_btn_idx: u16,
    pub delete_confirm_idx: u16,
    pub settings_state: ListState,
    pub roster_view: RosterView,
    pub settings_tab: SettingsTab,
    pub show_download_confirm: bool,
    pub is_deleting: bool,
    pub downloading_id: Option<String>,
    pub download_progress: f64,
    pub download_total_bytes: u64,
    pub download_current_bytes: u64,
    pub download_bytes_per_sec: f64,
    pub download_eta_seconds: u64,
    pub download_start_time: Option<std::time::Instant>,
    pub Core_engine: CoreEngine,
    pub _purpose: String,
    pub _active_model_id: Option<String>,
    pub _chat_history: Vec<(String, String)>,
    pub _chat_input: String,
    pub _chat_scroll: u16,
    pub _is_setup_complete: bool,
    pub _generation_tps: f64,
    pub cpu_usage: f32,
    pub mem_usage_gb: f32,
    pub live_pulse: Arc<::cluaiz_shared::hardware::telemetry::ObservableHardwareState>,

    // ── UI State ──
    pub is_dirty: bool,
    pub printed_logo: bool,
    pub rendered_actions_count: usize,
    pub stream_scroll_offset: u16,
    pub last_rendered_height: u16,
    pub current_block_id: String,
    pub interaction_mode: AppMode,
    pub palette_state: ListState,
    pub context_palette_state: ListState,
    pub input_buffer: String,
    pub chat_paste_buffer: Vec<String>,
    pub last_input_time: std::time::Instant,
    pub logo_index: usize,
    pub activity_stream: Vec<ActivityBlock>,
    pub auto_mount_triggered: bool,
    pub is_client_mode: bool,
}


impl AppState {
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }

    pub fn clean(&mut self) {
        self.is_dirty = false;
    }

    // ── 🌊 STREAM HELPERS ─────────────────────────────────────────────────

    pub fn purge_live_blocks(&mut self) {
        self.activity_stream.retain(|b| !b.is_live());
    }

    /// Opens the Model List as a live interactive block in the stream.
    pub fn open_registry(&mut self) {
        self.purge_live_blocks();
        self.activity_stream.push(ActivityBlock::MenuOpened("Model List".to_string()));
        self.activity_stream.push(ActivityBlock::ActiveRoster);
        self.active_app = MenuApp::Roster;
        self.roster_view = RosterView::List;
    }

    /// Opens Model Details for the currently selected roster item.
    pub fn open_details(&mut self) {
        // Remove any existing details block
        self.activity_stream.retain(|b| !matches!(b, ActivityBlock::ActiveDetails));

        if let Some(idx) = self.roster_state.selected() {
            if let Some(model) = self.sorted_models.get(idx) {
                let name = model.manifest.name.clone();
                let ctx  = model.manifest.context_window.clone();
                let stat = model.status.clone();
                self.activity_stream.push(ActivityBlock::ModelSelected {
                    name, ctx, status: stat,
                });
            }
        }
        self.activity_stream.push(ActivityBlock::ActiveDetails);
        self.roster_view = RosterView::Details;
    }

    /// Collapses the live block and returns to parent (Details → List, List → None).
    /// Optionally appends a frozen record (e.g. DownloadStarted).
    pub fn close_active_block(&mut self, extra_record: Option<ActivityBlock>) {
        match self.roster_view {
            RosterView::Details => {
                // ESC from Details → go back to List (restore ActiveRoster)
                self.activity_stream.retain(|b| !matches!(b, ActivityBlock::ActiveDetails));
                self.activity_stream.push(ActivityBlock::ActiveRoster);
                self.roster_view = RosterView::List;
            }
            RosterView::List => {
                // ESC from List → close registry entirely
                self.purge_live_blocks();
                self.active_app = MenuApp::None;
                self.roster_view = RosterView::List;
            }
        }
        if let Some(record) = extra_record {
            self.activity_stream.push(record);
        }
    }

    /// True if there is any live interactive block active right now.
    pub fn has_active_block(&self) -> bool {
        self.activity_stream.iter().any(|b| b.is_live())
    }

    /// Returns the current live block type (for height calculation).
    pub fn live_block_type(&self) -> Option<&ActivityBlock> {
        self.activity_stream.iter().rev().find(|b| b.is_live())
    }

    pub fn new(starting_state: Option<OsState>) -> Self {
        let hardware = ::cluaiz_shared::hardware::get_cluaiz_profile();

        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let ram_gb = sys.total_memory() as f64 / 1_073_741_824.0;
        
        let mut roster_state = TableState::default();
        roster_state.select(Some(0));
        let mut menu_state = ListState::default();
        menu_state.select(Some(0));
        let mut settings_state = ListState::default();
        settings_state.select(Some(0));

        // Initialize state
        let os_state = starting_state.unwrap_or(OsState::Dashboard);

        let live_pulse = ::cluaiz_shared::hardware::telemetry::get_pulse();
        // ── Detect if Native IPC daemon is already running via Named Pipe probe ──
        // The daemon uses Windows Named Pipe, NOT a TCP socket.
        let _is_client_mode: bool = {
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(r"\\.\pipe\cluaiz_engine_pipe")
            {
                Ok(_) => true,
                Err(_) => false,
            }
        };

        let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
        let mut _active_model_id = schema.chat_models.text.clone();

        Self {
            os_state,
            username: "cluaiz".to_string(),
            frame_counter: 0,
            _active_tab: Tab::All,
            active_app: MenuApp::None,
            _menu_app: MenuApp::None,
            hardware,
            ram_gb,
            sorted_models: Vec::new(),
            roster_state,
            menu_state,
            settings_state,
            roster_view: RosterView::List,
            settings_tab: SettingsTab::Main,
            roster_h_scroll: 0,
            details_btn_idx: 0,
            delete_confirm_idx: 0,
            show_download_confirm: false,
            is_deleting: false,
            downloading_id: None,
            download_progress: 0.0,
            download_total_bytes: 0,
            download_current_bytes: 0,
            download_bytes_per_sec: 0.0,
            download_eta_seconds: 0,
            download_start_time: None,
            Core_engine: CoreEngine::new(),
            _purpose: String::new(),
            _active_model_id,
            _chat_history: Vec::new(),
            _chat_input: String::new(),
            _chat_scroll: 0,
            _is_setup_complete: false,
            _generation_tps: 0.0,
            cpu_usage: 0.0,
            mem_usage_gb: 0.0,
            live_pulse,

            is_dirty: true,
            printed_logo: false,
            rendered_actions_count: 0,
            stream_scroll_offset: 0,
            last_rendered_height: 0,
            current_block_id: "BOOT".to_string(),
            interaction_mode: AppMode::Normal,
            palette_state: ListState::default(),
            context_palette_state: ListState::default(),
            input_buffer: String::new(),
            chat_paste_buffer: Vec::new(),
            last_input_time: std::time::Instant::now(),
            logo_index: crate::assets::logos::logo_gallery::LOGO_VARIANTS.len() - 1,
            activity_stream: Vec::new(),
            auto_mount_triggered: false,
            is_client_mode: _is_client_mode,
        }
    }

    pub fn handle_events(&mut self, rx: &mut tokio::sync::mpsc::UnboundedReceiver<engines::DownloadEvent>) {
        while let Ok(event) = rx.try_recv() {
            match event {
                engines::DownloadEvent::Progress(prog, _current, _total, _speed, _eta) => {
                    self.download_progress = prog as f64;
                }
                engines::DownloadEvent::Complete(id) => {
                    if id == "INITIAL_LOAD" {
                        self.sorted_models = engines::CoreRoster::get_recommendations(
                            &self.hardware.to_hardware_truth(), self.ram_gb
                        );
                    } else if self.downloading_id.as_ref() == Some(&id) {
                        let name = self.sorted_models.iter()
                            .find(|m| m.manifest.id == id)
                            .map(|m| m.manifest.name.clone())
                            .unwrap_or_else(|| id.clone());

                        self.downloading_id = None;
                        self.activity_stream.push(ActivityBlock::DownloadComplete(name));
                        self.sorted_models = engines::CoreRoster::get_recommendations(
                            &self.hardware.to_hardware_truth(), self.ram_gb
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

