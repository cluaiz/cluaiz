use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::RowAccessor;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

fn download_file(url: &str, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if output_path.exists() {
        println!("File already exists, skipping download: {:?}", output_path);
        return Ok(());
    }
    println!("Downloading {} -> {:?}", url, output_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let status = std::process::Command::new("curl")
        .args(&["-L", "-f", "-o", output_path.to_str().unwrap(), url])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to download file from {}", url).into());
    }
    Ok(())
}

fn get_existing_languages(output_root: &Path) -> std::collections::HashSet<String> {
    let mut langs = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir(output_root) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".txt") {
                    let lang = name.trim_end_matches(".txt").to_string();
                    langs.insert(lang);
                }
            }
        }
    }
    langs
}

fn load_existing_lexicon(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                map.insert(parts[0].to_lowercase(), parts[1].to_string());
            }
        }
    }
    map
}

fn write_lang_groups(
    lang_groups: HashMap<String, HashMap<String, String>>,
    output_root: &Path,
    existing_langs: &mut std::collections::HashSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(output_root)?;
    for (lang, word_map) in lang_groups {
        if word_map.is_empty() {
            continue;
        }
        let txt_path = output_root.join(format!("{}.txt", lang));
        let txt_file = File::create(&txt_path)?;
        let mut txt_writer = BufWriter::new(txt_file);
        let mut entries: Vec<(String, String)> = word_map.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (word, ipa) in &entries {
            writeln!(txt_writer, "{}\t{}", word, ipa)?;
        }
        txt_writer.flush()?;
        println!("Generated text lexicon under {:?}", txt_path);
        existing_langs.insert(lang);
    }
    Ok(())
}

fn process_omneity_parquet(
    file_path: &Path,
    lang_groups: &mut HashMap<String, HashMap<String, String>>,
    target_lang: Option<&str>,
    existing_langs: &std::collections::HashSet<String>,
    omneity_langs: &mut std::collections::HashSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Processing Omneity Labs dataset (Priority 1) from {:?}",
        file_path
    );
    let file = File::open(file_path)?;
    let reader = SerializedFileReader::new(file)?;

    // Print the schema to stdout
    let file_metadata = reader.metadata().file_metadata();
    println!("Omneity Parquet Schema: {:#?}", file_metadata.schema());

    let row_iter = reader.get_row_iter(None)?;
    let mut unique_langs = std::collections::HashSet::new();

    for record in row_iter {
        let row = record?;
        let lang = row.get_string(2).map(|s| s.to_string()).unwrap_or_default();
        if !lang.is_empty() {
            unique_langs.insert(lang.clone());
        }

        let text = row.get_string(0).map(|s| s.to_string()).unwrap_or_default();
        let ipa = row.get_string(1).map(|s| s.to_string()).unwrap_or_default();

        if text.is_empty() || ipa.is_empty() || lang.is_empty() {
            continue;
        }

        let norm_lang = lang.to_lowercase().replace('_', "-").trim().to_string();
        omneity_langs.insert(norm_lang.clone());

        if existing_langs.contains(&norm_lang) {
            continue;
        }

        if let Some(t_lang) = target_lang {
            if norm_lang != t_lang && !norm_lang.starts_with(&format!("{}-", t_lang)) {
                continue;
            }
        }

        let clean_word = text.trim().to_lowercase();
        let clean_ipa = ipa.trim().to_string();

        if !clean_word.is_empty() && !clean_ipa.is_empty() {
            lang_groups
                .entry(norm_lang)
                .or_default()
                .insert(clean_word, clean_ipa);
        }
    }

    println!(
        "Unique languages found in Omneity Parquet: {:?}",
        unique_langs
    );
    Ok(())
}

fn process_neurlang_parquet(
    file_path: &Path,
    lang_groups: &mut HashMap<String, HashMap<String, String>>,
    target_lang: Option<&str>,
    omneity_langs: &std::collections::HashSet<String>,
    output_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Processing Neurlang dataset (Priority 2) from {:?}",
        file_path
    );
    let file = File::open(file_path)?;
    let reader = SerializedFileReader::new(file)?;

    let mut lang_col: usize = 0;
    let mut word_col: usize = 1;
    let mut ipa_col: usize = 8;
    let mut votes_col: usize = 3;

    let schema = reader.metadata().file_metadata().schema();
    if schema.is_group() {
        let fields = schema.get_fields();
        for (idx, field) in fields.iter().enumerate() {
            let name = field.name().to_lowercase();
            if name == "language" || name == "lang" {
                lang_col = idx;
            } else if name == "headword" || name == "word" {
                word_col = idx;
            } else if name == "ipa" || name == "phonetic" {
                ipa_col = idx;
            } else if name == "votes" {
                votes_col = idx;
            }
        }
    }

    let row_iter = reader.get_row_iter(None)?;

    for record in row_iter {
        let row = record?;
        let language = row
            .get_string(lang_col)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let headword = row
            .get_string(word_col)
            .map(|s| s.to_string())
            .unwrap_or_default();

        let _votes = row
            .get_int(votes_col)
            .map(|v| v as i64)
            .or_else(|_| row.get_long(votes_col))
            .unwrap_or(0);

        let ipa = row
            .get_string(ipa_col)
            .map(|s| s.to_string())
            .unwrap_or_default();

        if language.is_empty() || headword.is_empty() || ipa.is_empty() {
            continue;
        }

        let norm_lang = language.to_lowercase().replace('_', "-").trim().to_string();

        if omneity_langs.contains(&norm_lang) {
            continue;
        }

        if let Some(t_lang) = target_lang {
            if norm_lang != t_lang && !norm_lang.starts_with(&format!("{}-", t_lang)) {
                continue;
            }
        }

        let clean_word = headword.trim().to_lowercase();
        let clean_ipa = ipa.trim().to_string();

        if !clean_word.is_empty() && !clean_ipa.is_empty() {
            let lang_map = lang_groups.entry(norm_lang.clone()).or_insert_with(|| {
                let txt_path = output_root.join(format!("{}.txt", norm_lang));
                load_existing_lexicon(&txt_path)
            });

            if !lang_map.contains_key(&clean_word) {
                lang_map.insert(clean_word, clean_ipa);
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let mut target_lang: Option<String> = None;
    let mut no_neurlang = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--lang" => {
                if i + 1 < args.len() {
                    target_lang = Some(args[i + 1].to_lowercase());
                    i += 1;
                }
            }
            "--no-neurlang" => {
                no_neurlang = true;
            }
            _ => {
                // Ignore other arguments for backward compatibility or simple triggers
            }
        }
        i += 1;
    }

    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let output_root = env
        .local_dir
        .parent()
        .map(|p| p.join("assets"))
        .unwrap_or_else(|| PathBuf::from("assets"))
        .join("ipa_dictionary");

    if let Some(ref l) = target_lang {
        println!("Target Language: {}", l);
    }
    if no_neurlang {
        println!("Skip Neurlang: true");
    }

    // Scan existing languages on disk
    let mut existing_langs = get_existing_languages(&output_root);
    println!("Existing languages on disk: {:?}", existing_langs);

    let temp_dir = env.local_dir.join("temp_lexicons");
    fs::create_dir_all(&temp_dir)?;

    let omneity_url = "https://huggingface.co/datasets/omneity-labs/ipa-dict/resolve/refs%2Fconvert%2Fparquet/default/train/0000.parquet";
    let omneity_path = temp_dir.join("omneity_0000.parquet");
    download_file(omneity_url, &omneity_path)?;

    let mut lang_groups: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut omneity_langs = std::collections::HashSet::new();
    process_omneity_parquet(
        &omneity_path,
        &mut lang_groups,
        target_lang.as_deref(),
        &existing_langs,
        &mut omneity_langs,
    )?;
    write_lang_groups(lang_groups, &output_root, &mut existing_langs)?;

    if !no_neurlang {
        let neurlang_shards = [
            "https://huggingface.co/datasets/neurlang/ipa-lexicon-4v0-7M/resolve/refs%2Fconvert%2Fparquet/default/train/0000.parquet",
            "https://huggingface.co/datasets/neurlang/ipa-lexicon-4v0-7M/resolve/refs%2Fconvert%2Fparquet/default/train/0001.parquet",
            "https://huggingface.co/datasets/neurlang/ipa-lexicon-4v0-7M/resolve/refs%2Fconvert%2Fparquet/default/train/0002.parquet",
            "https://huggingface.co/datasets/neurlang/ipa-lexicon-4v0-7M/resolve/refs%2Fconvert%2Fparquet/default/train/0003.parquet",
        ];

        for (idx, url) in neurlang_shards.iter().enumerate() {
            let neurlang_path = temp_dir.join(format!("neurlang_{:04}.parquet", idx));
            if let Err(e) = download_file(url, &neurlang_path) {
                println!("Skipping shard {} (failed/nonexistent): {}", idx, e);
                continue;
            }
            let mut shard_lang_groups: HashMap<String, HashMap<String, String>> = HashMap::new();
            process_neurlang_parquet(
                &neurlang_path,
                &mut shard_lang_groups,
                target_lang.as_deref(),
                &omneity_langs,
                &output_root,
            )?;
            write_lang_groups(shard_lang_groups, &output_root, &mut existing_langs)?;
        }
    }

    println!("Process complete!");
    Ok(())
}


