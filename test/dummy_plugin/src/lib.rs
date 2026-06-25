//! Dummy PoC Plugin for testing CEL FFI Architecture

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum PayloadType {
    Json,
    WasmBinary,
    RawBytes,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ExtensionPayload {
    pub payload_type: PayloadType,
    pub data_ptr: *const u8,
    pub data_len: usize,
}

#[no_mangle]
pub extern "C" fn execute_cel(payload: ExtensionPayload) -> *mut u8 {
    // 1. Read the incoming payload
    let incoming_bytes = unsafe {
        std::slice::from_raw_parts(payload.data_ptr, payload.data_len)
    };
    
    let query_str = String::from_utf8_lossy(incoming_bytes);
    println!("🔌 Dummy Plugin Received CEL Payload: {}", query_str);

    // 2. Perform mock native execution
    let result_string = format!("SUCCESS: Processed '{}' in 0.05ms native speed", query_str);
    
    // 3. Return as a CString pointer
    let c_string = std::ffi::CString::new(result_string).unwrap();
    
    // into_raw hands over memory management to the caller
    c_string.into_raw() as *mut u8
}
