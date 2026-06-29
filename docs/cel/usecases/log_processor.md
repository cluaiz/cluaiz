---
title: CEL Log Processor
description: Rapid ingestion, filtering, and time-windowing of server logs using CEL Native SDK.
---

# CEL Usecase: High-Throughput Log Processing

Processing high-velocity server logs inside Python or JS introduces Garbage Collection pauses and object allocation overhead. By passing the raw log stream into the **CEL Native SDK**, Rust handles the parsing, slicing, and memory drops instantaneously.

## The Pipeline

This pipeline:
1. Takes a massive JSON array of logs (e.g., 100,000 lines).
2. Uses `foreach` to iterate through them.
3. Keeps only `"ERROR"` logs.
4. Truncates stale logs using `time_window`.
5. Sends the filtered subset to a monitoring plugin.

```cel
let $logs = ?1

foreach ($log in $logs) {
    if ($log.level == "ERROR") {
        let $recent_error = $log -> time_window(10m)
        if ($recent_error != null) {
            use plugin::pagerduty -> invoke(alert, data: $recent_error)
        }
    }
}
```

## Executing from Rust Native SDK

When using the SDK natively inside a Rust host service, this data transfer is 100% zero-copy. The `$logs` array remains in the host's memory, and the CEL engine iterates over it using pointers.

```rust
use cluaiz_sdk::{execute, ExtensionPayload, PayloadType, Transpiler};
use serde_json::json;

fn process_server_logs() {
    let cel_script = r#"
        let $logs = ?1

        foreach ($log in $logs) {
            if ($log.level == "ERROR") {
                let $recent_error = $log -> time_window(10m)
                if ($recent_error != null) {
                    use plugin::pagerduty -> invoke(alert, data: $recent_error)
                }
            }
        }
    "#;

    // A massive 50MB array of logs
    let huge_log_array = json!([
        { "level": "INFO", "timestamp": 1718290000, "msg": "Started" },
        { "level": "ERROR", "timestamp": 1718290050, "msg": "OOM Killed" },
        // ... 100,000 more lines ...
    ]);

    // Bincode transpilation (0.05ms) instead of JSON parsing overhead
    let payload_bytes = Transpiler::to_binary_payload(&huge_log_array).unwrap();
    let payload = ExtensionPayload::new(PayloadType::Bincode, &payload_bytes);

    // Executes in Rust Tokio Runtime, dropping unneeded logs instantly
    let _ = execute(cel_script, vec![payload]);
}
```

## Architectural Data Flow

```mermaid
flowchart TD
    A["Rust Host: execute(script)"] --> B{"CEL Engine"}
    
    B -->|?1 (Bincode)| C["foreach ($log in $logs)"]
    
    C --> D{"if ($log.level == 'ERROR')"}
    D -->|False| E["Drop instantly (Zero GC)"]
    D -->|True| F["time_window(10m)"]
    
    F -->|Stale| E
    F -->|Recent| G["Plugin: pagerduty"]
    
    G -->|invoke(alert)| H["Alert Sent"]
```
