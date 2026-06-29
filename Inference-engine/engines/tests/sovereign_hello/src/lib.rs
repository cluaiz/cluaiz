use std::slice;
use std::str;

// 🏹 Sovereign ABI: cluaiz-OS requires a malloc export to pass strings safely
#[no_mangle]
pub extern "C" fn malloc(size: usize) -> *mut u8 {
    let mut vec = Vec::with_capacity(size);
    let ptr = vec.as_mut_ptr();
    std::mem::forget(vec);
    ptr
}

#[no_mangle]
pub extern "C" fn run(ptr: *mut u8, len: usize) -> *mut u8 {
    // 1. Read input from the kernel
    let input = unsafe {
        let bytes = slice::from_raw_parts(ptr, len);
        str::from_utf8(bytes).unwrap_or("INVALID_INPUT")
    };

    // 🧠 Logic: Greet the Founder
    let response = format!("Hello Cluaiz Technologies! cluaiz-OS received your intent: '{}'. Sandbox is LOCKED. Neural Pulse sent.", input);
    
    // 2. Return the result as a null-terminated string (Sovereign Format)
    let mut resp_bytes = response.into_bytes();
    resp_bytes.push(0); // Null terminator
    let ptr = resp_bytes.as_mut_ptr();
    std::mem::forget(resp_bytes);
    ptr
}
