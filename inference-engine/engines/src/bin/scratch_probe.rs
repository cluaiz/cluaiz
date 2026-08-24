use std::env;
use engines::models::GgufProber;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: probe <file.gguf>");
        return;
    }
    
    let path = std::path::Path::new(&args[1]);
    match GgufProber::probe(&path) {
        Ok((metadata, _, _)) => {
            for (k, v) in metadata {
                if !v.starts_with("[StringArray") && !v.starts_with("[PrimitiveArray") {
                    println!("{}: {}", k, v);
                }
            }
        }
        Err(e) => println!("Error: {}", e),
    }
}
