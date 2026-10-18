//! Embeds every letter-to-sound rule file in `src/audio/tts/data/g2p_rules/` (one JSON per
//! language), so adding a language is adding a file: no code lists them.

use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/audio/tts/data/g2p_rules");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut code = String::from("pub const RULE_FILES: &[(&str, &str)] = &[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        code.push_str(&format!("    ({name:?}, include_str!({:?})),\n", path.display().to_string()));
    }
    code.push_str("];\n");
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("g2p_rule_files.rs");
    std::fs::write(out, code).expect("write g2p_rule_files.rs");
}
