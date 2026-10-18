//! Embeds every per-language JSON file of the TTS data folders (letter-to-sound rules in
//! `data/g2p_rules/`, romanized-writing rules in `data/transliteration/`, number words in
//! `data/numbers/`), so adding a language is
//! adding a file: no code lists them.

use std::path::Path;

/// (folder under src/audio/tts/data, generated constant)
const FOLDERS: [(&str, &str); 3] = [
    ("g2p_rules", "RULE_FILES"),
    ("transliteration", "TRANSLITERATION_FILES"),
    ("numbers", "NUMBER_FILES"),
];

fn main() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/audio/tts/data");
    let mut code = String::new();
    for (folder, constant) in FOLDERS {
        let dir = data.join(folder);
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
        code.push_str(&format!("pub const {constant}: &[(&str, &str)] = &[\n"));
        for path in &files {
            println!("cargo:rerun-if-changed={}", path.display());
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            code.push_str(&format!("    ({name:?}, include_str!({:?})),\n", path.display().to_string()));
        }
        code.push_str("];\n");
    }
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("g2p_rule_files.rs");
    std::fs::write(out, code).expect("write g2p_rule_files.rs");
}
