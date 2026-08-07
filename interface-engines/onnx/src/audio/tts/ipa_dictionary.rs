use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// Compile a raw lexicon.txt (word<TAB>ipa) file into a high-performance lexicon.bin
pub fn compile_txt_to_bin(txt_path: &Path, bin_path: &Path) -> Result<(), std::io::Error> {
    let content = std::fs::read_to_string(txt_path)?;
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            entries.push((parts[0].to_lowercase(), parts[1].to_string()));
        } else {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                entries.push((parts[0].to_lowercase(), parts[1..].join(" ")));
            }
        }
    }

    let bin_file = File::create(bin_path)?;
    let mut bin_writer = BufWriter::new(bin_file);

    let count = entries.len() as u32;
    bin_writer.write_all(&count.to_le_bytes())?;

    for (word, ipa) in entries {
        let word_bytes = word.as_bytes();
        let word_len = word_bytes.len() as u32;
        bin_writer.write_all(&word_len.to_le_bytes())?;
        bin_writer.write_all(word_bytes)?;

        let ipa_bytes = ipa.as_bytes();
        let ipa_len = ipa_bytes.len() as u32;
        bin_writer.write_all(&ipa_len.to_le_bytes())?;
        bin_writer.write_all(ipa_bytes)?;
    }

    bin_writer.flush()?;
    Ok(())
}

/// Load a compiled lexicon.bin into memory
pub fn load_bin_lexicon(bin_path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
    let mut file = File::open(bin_path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    if buf.len() < 4 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid binary lexicon file length",
        ));
    }

    let mut cursor = 0;
    let count = u32::from_le_bytes(buf[0..4].try_into().unwrap());
    cursor += 4;

    let mut dict = HashMap::with_capacity(count as usize);

    for _ in 0..count {
        if cursor + 4 > buf.len() {
            break;
        }
        let word_len = u32::from_le_bytes(buf[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;

        if cursor + word_len > buf.len() {
            break;
        }
        let word = std::str::from_utf8(&buf[cursor..cursor + word_len])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
            .to_string();
        cursor += word_len;

        if cursor + 4 > buf.len() {
            break;
        }
        let ipa_len = u32::from_le_bytes(buf[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;

        if cursor + ipa_len > buf.len() {
            break;
        }
        let ipa = std::str::from_utf8(&buf[cursor..cursor + ipa_len])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
            .to_string();
        cursor += ipa_len;

        dict.insert(word, ipa);
    }

    Ok(dict)
}

fn download_from_url(url: &str, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = std::process::Command::new("curl")
        .args(&["-L", "-f", "-o", output_path.to_str().unwrap(), url])
        .status()?;
    if !status.success() {
        return Err("Failed to download".into());
    }
    Ok(())
}

/// Dynamic G2P compiler and loader entry point
pub fn load_or_compile_lexicon(model_dir: &Path) -> HashMap<String, String> {
    let lang = super::g2p::get_model_language(model_dir);
    let txt_path = model_dir.join("lexicon.txt");
    let bin_path = model_dir.join("lexicon.bin");

    // 1. If both are missing, copy lexicon.txt from assets/ipa_dictionary/ or download from GitHub Raw
    if !txt_path.exists() && !bin_path.exists() {
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        let mut assets_dir = env.local_dir.parent().map(|p| p.join("assets")).unwrap_or_else(|| PathBuf::from("assets"));
        if !assets_dir.exists() {
            assets_dir = env.global_dir.join("assets");
        }
        let dict_dir = assets_dir.join("ipa_dictionary");

        // Try exact match, e.g. hi-in.txt
        let mut assets_txt = dict_dir.join(format!("{}.txt", lang));

        // If exact match doesn't exist, try prefix match (e.g. hi-in falls back to hi.txt)
        if !assets_txt.exists() && lang.contains('-') {
            if let Some(prefix) = lang.split('-').next() {
                let fallback_txt = dict_dir.join(format!("{}.txt", prefix));
                if fallback_txt.exists() {
                    assets_txt = fallback_txt;
                }
            }
        }

        let mut copied = false;
        if assets_txt.exists() {
            if std::fs::copy(&assets_txt, &txt_path).is_ok() {
                eprintln!("📋 [G2P Router] Copied lexicon.txt from assets ({:?})", assets_txt.file_name().unwrap_or_default());
                copied = true;
            }
        }

        if !copied {
            eprintln!("🌐 [G2P Router] Local assets not found. Attempting to download lexicon from GitHub Raw for lang '{}'...", lang);
            let github_url = format!("https://raw.githubusercontent.com/cluaiz/cluaiz/main/assets/ipa_dictionary/{}.txt", lang);
            if download_from_url(&github_url, &txt_path).is_err() && lang.contains('-') {
                if let Some(prefix) = lang.split('-').next() {
                    let fallback_url = format!("https://raw.githubusercontent.com/cluaiz/cluaiz/main/assets/ipa_dictionary/{}.txt", prefix);
                    let _ = download_from_url(&fallback_url, &txt_path);
                }
            }
        }
    }

    // 2. Check if lexicon.txt was modified (newer than lexicon.bin)
    if txt_path.exists() && bin_path.exists() {
        let txt_meta = std::fs::metadata(&txt_path);
        let bin_meta = std::fs::metadata(&bin_path);
        if let (Ok(txt_m), Ok(bin_m)) = (txt_meta, bin_meta) {
            if let (Ok(txt_time), Ok(bin_time)) = (txt_m.modified(), bin_m.modified()) {
                if txt_time > bin_time {
                    eprintln!("⚙️ [G2P Router] Modified lexicon.txt detected. Recompiling to lexicon.bin...");
                    let _ = compile_txt_to_bin(&txt_path, &bin_path);
                }
            }
        }
    } else if txt_path.exists() && !bin_path.exists() {
        eprintln!("⚙️ [G2P Router] lexicon.txt found but lexicon.bin missing. Compiling...");
        let _ = compile_txt_to_bin(&txt_path, &bin_path);
    }

    // 3. Load from lexicon.bin if present, fallback to raw text parse
    if bin_path.exists() {
        match load_bin_lexicon(&bin_path) {
            Ok(dict) => {
                eprintln!(
                    "🚀 [G2P Router] Loaded {} entries from binary lexicon.bin in nanoseconds",
                    dict.len()
                );
                return dict;
            }
            Err(e) => {
                eprintln!(
                    "⚠️ [G2P Router] Failed to load binary lexicon, falling back to text: {:?}",
                    e
                );
            }
        }
    }

    // Fallback parsing of lexicon.txt
    let mut dict = HashMap::new();
    if txt_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&txt_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let word = parts[0].to_lowercase();
                    let ipa = parts[1..].join(" ");
                    dict.insert(word, ipa);
                }
            }
        }
    }

    dict
}
