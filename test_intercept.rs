fn main() {
    let mut buffer = String::new();
    let tokens = ["use", " extension", "::", "cl", "ua", "iz", "-search", "\n[DONE]\n"];
    for token in tokens {
        buffer.push_str(token);
        let triggers = ["use extension::", "use plugin::"];
        for trigger in triggers {
            if let Some(idx) = buffer.find(trigger) {
                let remainder = &buffer[idx + trigger.len()..];
                if let Some(end_idx) = remainder.find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
                    if end_idx > 0 {
                        let target_name = &remainder[..end_idx];
                        println!("INTERCEPTED: {}", target_name);
                        return;
                    }
                }
            }
        }
        println!("YIELD: {}", token);
    }
}
