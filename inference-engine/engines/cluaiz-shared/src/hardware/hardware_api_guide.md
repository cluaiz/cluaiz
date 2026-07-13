# 🏛️ Independent Hardware Interface Guide (API Guide)

Ye document ek "API Guide" hai jo batata hai ki cluaiz Backend Engine (`archer-shared`) ko Frontend, CLI, ya kisi doosre software se kaise call kiya jayega. **(No `cargo test` required in production!)**

---

## 1. ⚙️ Static Hardware Identity (The "One-Time" Check)
**Kab use karein:** Jab system boot ho, ya jab tumhe check karna ho ki kaunsa GPU laga hai, uske liye kaunsa driver download karna hai, ya total SSD kitni hai.

**Kaise Call Karein (In Rust CLI/Frontend):**
```rust
use cluaiz_shared::hardware::system_control::HardwareOrchestrator;

// Ye line engine ko start karegi aur directly system_control.json bana degi.
let static_data = HardwareOrchestrator::start().unwrap();

// Ab tum is data ko UI me dikha sakte ho ya driver download logic me daal sakte ho
let gpu_name = &static_data.silicon_truth.accelerators.gpus[0].vendor;
println!("Detected GPU for Driver Download: {}", gpu_name);
```
*(CLI bas ek baar ye function call karegi, koi test command chalane ki zaroorat nahi hai).*

---

## 2. ⚡ Live Pulse / Ghost Observer (The "Real-Time" Stream)
**Kab use karein:** Jab tumhe Terminal par ek live dashboard (Task Manager jaisa) dikhana ho, ya AI Model ko batana ho ki "Bhai GPU ka temperature 90°C ho gaya hai, thoda slow ho ja".

**Kaise Call Karein (In Rust CLI/Frontend):**
```rust
use cluaiz_shared::hardware::system_performance::SystemPerformanceLive;

// Ye function background me ek thread chalu kar dega jo har 500ms me update hoga.
let live_pulse = SystemPerformanceLive::start_background_stream();

// Tumhara CLI ya UI ek loop chalayega jo screen ko update karega:
loop {
    let current_data = live_pulse.read().unwrap();
    
    // UI rendering logic
    println!("Live GPU Temp: {}°C", current_data.gpu.temperature_c);
    println!("Live CPU %: {}%", current_data.cpu.utilization_pct);
    
    // UI 100ms me refresh hoga, bina kisi lag ke!
    std::thread::sleep(std::time::Duration::from_millis(100));
}
```

---

## 3. 🖥️ The TUI Vision (Terminal User Interface)
Asali user kabhi command line se tests nahi chalayega. Humara final implementation aisa dikhega:

1. **`cluaiz hardware`** -> CLI `system_control.rs` ko call karegi aur hardware ki poori kundli screen par print kar degi.
2. **`cluaiz monitor`** -> CLI `system_performance_live.rs` ko call karegi aur ek **Live Hacker-Style TUI** (Progress bars, changing colors) khol degi jo seedha Atomic memory se data padhega.
