//! The CXP Trait (cluaiz Extension Protocol)
//! This defines the C-ABI compatible interface for plugins across all formats.

#[repr(C)]
#[derive(Clone, Copy)]
pub enum PayloadType {
    Json,
    Cdql,
    WasmBinary,
    RawBytes,
    Bincode, // Strict Binary Transpilation target
}

#[repr(C)]
pub struct ExtensionPayload {
    pub payload_type: PayloadType,
    pub data_ptr: *const u8,
    pub data_len: usize,
}

impl ExtensionPayload {
    /// Creates a new ExtensionPayload from a byte slice.
    pub fn new(payload_type: PayloadType, bytes: &[u8]) -> Self {
        Self {
            payload_type,
            data_ptr: bytes.as_ptr(),
            data_len: bytes.len(),
        }
    }

    /// Safely reconstructs the byte slice from the C-ABI pointers.
    /// # Safety
    /// The caller must ensure that the memory `data_ptr` points to is valid for `data_len` bytes
    /// and outlives the returned slice.
    pub unsafe fn as_bytes(&self) -> &[u8] {
        std::slice::from_raw_parts(self.data_ptr, self.data_len)
    }
}

/// The Binary Transpiler converts internal ASTs and JSON structures into 
/// strictly typed binary representations (Bincode) to achieve 0.05ms native FFI transfers
/// without JSON parsing overhead.
pub struct Transpiler;

impl Transpiler {
    pub fn to_binary_payload<T: serde::Serialize>(data: &T) -> Result<Vec<u8>, String> {
        bincode::serialize(data).map_err(|e| format!("Transpilation to binary failed: {}", e))
    }

    pub fn from_binary_payload<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, String> {
        bincode::deserialize(bytes).map_err(|e| format!("Binary to struct transpilation failed: {}", e))
    }
}

// The engine MUST call this function after reading memory allocated by a C-FFI Plugin.
// Without this, any pointer returned by the plugin across the boundary will become a RAM leak.
extern "C" {
    pub fn cluaiz_free_payload(ptr: *mut u8, len: usize);
}
