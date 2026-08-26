use inference_cel::parse_cel;

#[test]
fn test_cel_ffi_architecture() {
    println!("🧪 [Test CEL FFI] Starting Architectural Verification...");

    // 1. The User / AI generates a CEL Command
    let cel_string = "use plugin::dummy_plugin -> process('Hello Native WASM World!')";
    println!("🧠 [Core Engine] Received CEL Command: {}", cel_string);

    // 2. The Engine's Universal Parser converts it into an execution plan
    let plan = parse_cel(cel_string);
    assert!(plan.is_ok(), "Failed to parse CEL: {:?}", plan.err());
    
    println!("✅ [Core Engine] Successfully Parsed CEL Expression.");
    println!("🧪 [Test CEL FFI] Diagnostic complete!");
}
