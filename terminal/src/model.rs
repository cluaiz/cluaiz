//! Caller-supplied display context and bounded presentation preferences.

/// Header content supplied by the client; the UI does not probe models or workspaces.
#[derive(Clone, Default)]
pub struct Metadata {
    pub title: String,
    pub workspace: String,
    pub status: String,
    /// Caller-owned brand art, used only when it fits without clipping the composer.
    pub brand: &'static str,
    pub compact_brand: &'static str,
    /// Optional click frame. It must preserve the primary artwork's row/cell dimensions.
    pub brand_blink: &'static str,
}

/// Resource limits, independent of a model's token/context budget.
#[derive(Clone, Copy)]
pub struct Limits {
    pub input_bytes: usize,
    pub retained_entries: usize,
    pub entry_bytes: usize,
    pub edit_history: usize,
}

impl Default for Limits {
    fn default() -> Self {
        const INPUT_BYTES: usize = 128 * 1024;
        const RETAINED_ENTRIES: usize = 128;
        const ENTRY_BYTES: usize = 128 * 1024;
        const EDIT_HISTORY: usize = 32;
        Self {
            input_bytes: INPUT_BYTES,
            retained_entries: RETAINED_ENTRIES,
            entry_bytes: ENTRY_BYTES,
            edit_history: EDIT_HISTORY,
        }
    }
}
