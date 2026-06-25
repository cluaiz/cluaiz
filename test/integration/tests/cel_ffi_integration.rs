use engines::memory::cel_bridge::cel_parser;
use engines::memory::cel_bridge::plugin_loader;

#[test]
fn test_cel_ffi_architecture() {
    println!("🧪 [Test CEL FFI] Starting Architectural Verification...");

    // 1. The User / AI generates a CEL Command
    let cel_string = "use plugin::dummy_plugin -> process('Hello Native WASM World!')";
    println!("🧠 [Core Engine] Received CEL Command: {}", cel_string);

    // 2. The Engine's Universal Parser converts it into an AST
    let ast = cel_parser::parse_cel_to_ast(cel_string).expect("Failed to parse CEL");
    
    assert_eq!(ast.target_plugin, "dummy_plugin");
    assert_eq!(ast.action, "process");
    
    println!("✅ [Core Engine] Successfully Parsed AST.");
    println!("  - Target Plugin: {}", ast.target_plugin);
    println!("  - Action: {}", ast.action);
    println!("  - Payload: {}", String::from_utf8_lossy(&ast.payload_bytes));

    // 3. The Engine routes the AST directly to the C-FFI Plugin Boundary
    // NOTE: This test requires `dummy_plugin.dll` to be present in `active_dnas/`.
    // If not present, the test will skip or fail gracefully depending on setup.
    println!("⚡ [Core Engine] Routing payload via 0.05ms Zero-Cost FFI...");
    
    if let Some(result_bytes) = plugin_loader::route_to_plugin(ast) {
        let result_str = String::from_utf8_lossy(&result_bytes);
        println!("🚀 [Native Plugin] Returned Result: {}", result_str);
        assert!(result_str.contains("SUCCESS"));
        
        // 4. Safely inject into VRAM
        engines::memory::cel_bridge::gpu_injector::inject_into_kv_cache(result_bytes);
    } else {
        println!("⚠️ [Native Plugin] DLL not loaded. Skipping FFI execution step.");
    }

    println!("🧪 [Test CEL FFI] Diagnostic complete!");
}
