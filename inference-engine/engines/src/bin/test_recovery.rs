use cluaiz_shared::HardwareGovernor;

fn main() {
    println!("🧪 [Test] Attempting to load cluaiz Truth...");
    match HardwareGovernor::load_system_control() {
        Ok(control) => {
            println!("✅ [Test] Load Success! cluaiz Root: {}", control.context.cluaiz_root);
        },
        Err(e) => {
            println!("❌ [Test] Load Failed (as expected if recovery failed): {}", e);
        }
    }
}
