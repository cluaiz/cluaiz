use anyhow::{Result, anyhow};
use std::path::PathBuf;
use cluaiz_shared::environment::EnvironmentManager;

pub async fn execute(
    opt_type: Option<String>,
    opt_id: Option<String>,
    opt_key: Option<String>,
    opt_val: Option<String>,
) -> Result<()> {
    let mut final_type = opt_type;
    let mut final_id = opt_id;
    let mut final_key = opt_key.clone();
    let mut final_val = opt_val.clone();

    // Direct command mode: all args pre-supplied — save once and exit, no dropdown loop
    let direct_mode = opt_key.is_some() && opt_val.is_some();

    // Type Selection
    if final_type.is_none() {
        final_type = inquire::Select::new(
            "Select Component Type:",
            vec!["extension".to_string(), "plugin".to_string(), "skill".to_string(), "mcp".to_string()],
        ).prompt().ok();
    }
    let comp_type = final_type.ok_or_else(|| anyhow!("Component type required"))?;

    let env = EnvironmentManager::current();
    let base_dir = env.global_dir.join(format!("{}s", comp_type));

    // ID Selection
    if final_id.is_none() {
        let mut options = Vec::new();
        if base_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&base_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        options.push(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }
        if options.is_empty() {
            return Err(anyhow!("No {}s installed to configure.", comp_type));
        }
        final_id = inquire::Select::new(&format!("Select {} ID:", comp_type), options).prompt().ok();
    }
    let comp_id = final_id.ok_or_else(|| anyhow!("Component ID required"))?;

    let comp_dir = base_dir.join(&comp_id);
    let file_name = if comp_type == "skill" {
        "SKILL.md".to_string()
    } else {
        format!("manifest-{}.yaml", comp_type)
    };
    let file_path = comp_dir.join(&file_name);

    if !file_path.exists() {
        return Err(anyhow!("Component not found at {}", file_path.display()));
    }

    let content = std::fs::read_to_string(&file_path)?;
    let yaml_part = if comp_type == "skill" && content.starts_with("---\n") {
        if let Some(end_idx) = content[4..].find("---\n") {
            content[4..end_idx + 4].to_string()
        } else {
            content.clone()
        }
    } else {
        content.clone()
    };

    let yaml: serde_yaml::Value = serde_yaml::from_str(&yaml_part).unwrap_or(serde_yaml::Value::Null);
    
    loop {
        
        // Key Selection based on configuration_schema
        if final_key.is_none() {
            let mut keys = Vec::new();
            if let Some(map) = yaml.as_mapping() {
                if let Some(settings) = map.get(&serde_yaml::Value::String("settings".to_string())) {
                    if let Some(settings_map) = settings.as_mapping() {
                        for (k, _) in settings_map {
                            if let Some(k_str) = k.as_str() {
                                keys.push(k_str.to_string());
                            }
                        }
                    }
                }
            }
            let mut keys_with_exit = keys.clone();
            keys_with_exit.push("Exit / Back".to_string());
            match inquire::Select::new("Select Setting to configure:", keys_with_exit).prompt().ok() {
                Some(c) if c == "Exit / Back" => break,
                Some(c) => { final_key = Some(c); }
                None => break, // User pressed Esc/Ctrl+C — exit cleanly
            }
        }
        
        let key = final_key.ok_or_else(|| anyhow!("Key path required"))?;
        // Remove legacy prefixes if user types them out of habit
        let key = key.replace("settings.", "").replace("", "");
    
        // Value Prompt
        let mut default_val: String = String::new();
        let mut is_enum: bool = false;
        let mut enum_options: Vec<String> = Vec::new();
        let mut expected_type: String = String::new();
    
        if let Some(map) = yaml.as_mapping() {
            if let Some(schema) = map.get(&serde_yaml::Value::String("settings".to_string())) {
                if let Some(schema_map) = schema.as_mapping() {
                    if let Some(field) = schema_map.get(&serde_yaml::Value::String(key.clone())) {
                        if let Some(field_map) = field.as_mapping() {
                            // Extract default
                            if let Some(def) = field_map.get(&serde_yaml::Value::String("default".to_string())) {
                                if let Some(d_str) = def.as_str() { default_val = d_str.to_string(); }
                            }
                            // Check type
                            if let Some(typ) = field_map.get(&serde_yaml::Value::String("type".to_string())) {
                                if let Some(t_str) = typ.as_str() {
                                    expected_type = t_str.to_string();
                                    if t_str == "enum" {
                                        is_enum = true;
                                        if let Some(opts) = field_map.get(&serde_yaml::Value::String("options".to_string())) {
                                            if let Some(seq) = opts.as_sequence() {
                                                for o in seq {
                                                    if let Some(o_str) = o.as_str() {
                                                        enum_options.push(o_str.to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    
        if final_val.is_none() {
            if is_enum && !enum_options.is_empty() {
                let mut current_idx = 0;
                let display_options: Vec<String> = enum_options.iter().enumerate().map(|(i, o)| {
                    if o == &default_val {
                        current_idx = i;
                        format!("{} (current)", o)
                    } else {
                        o.clone()
                    }
                }).collect();
                
                let prompt_msg = format!("Select value for {}:", key);
                let p = inquire::Select::new(&prompt_msg, display_options.clone());
                if let Ok(selected) = p.with_starting_cursor(current_idx).prompt() {
                    if let Some(idx) = display_options.iter().position(|r| r == &selected) {
                        final_val = Some(enum_options[idx].clone());
                    }
                }
            } else {
                let prompt_str = format!("Enter value for {}:", key);
                let p = inquire::Text::new(&prompt_str);
                let p = if !default_val.is_empty() { p.with_default(&default_val) } else { p };
                final_val = p.prompt().ok();
            }
        }
        let value_str = final_val.ok_or_else(|| anyhow!("Value required"))?;

        // Read manifest as raw string — update ONLY the `default:` field for this key
        // This preserves the original YAML formatting (inline `{ }` style or block style)
        let manifest_content = std::fs::read_to_string(&file_path)?;
        let updated_content = update_setting_default(&manifest_content, &key, &value_str);
        std::fs::write(&file_path, updated_content.as_bytes())?;

        // Invalidate the .bin cache by deleting it — engine will regenerate it on next run
        // from the freshly-updated YAML. This ensures stale cache never serves old settings.
        let bin_path = file_path.with_extension("bin");
        if bin_path.exists() {
            let _ = std::fs::remove_file(&bin_path);
        }

        println!("✅ [{}] {} → \"{}\" saved.", comp_id, key, value_str);

        if direct_mode {
            // Direct command mode — all args were pre-supplied, exit after one save
            break;
        }
        // Interactive (dropdown) mode — loop back to key selection automatically
        final_key = None;
        final_val = None;
    }

    Ok(())
}

/// Update only the `default:` field for a specific settings key in a raw YAML string,
/// preserving all original formatting — works for both inline `{ }` and block style.
fn update_setting_default(content: &str, target_key: &str, new_value: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut in_settings = false;
    let mut in_target_key_block = false;
    let mut target_key_base_indent = 0usize;
    let mut done = false;
    let had_trailing_newline = content.ends_with('\n');

    for line in content.lines() {
        if done {
            result.push(line.to_string());
            continue;
        }

        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        if !in_settings {
            if trimmed == "settings:" {
                in_settings = true;
            }
            result.push(line.to_string());
            continue;
        }

        // Left settings block (back to a top-level non-empty key)
        if indent == 0 && !trimmed.is_empty() {
            in_settings = false;
            in_target_key_block = false;
            result.push(line.to_string());
            continue;
        }

        if trimmed.is_empty() {
            result.push(line.to_string());
            continue;
        }

        // Check if this is the target key line inside settings
        let is_target = trimmed == format!("{}:", target_key).as_str()
            || trimmed.starts_with(&format!("{}: ", target_key))
            || trimmed.starts_with(&format!("{}:{{", target_key));

        if is_target && !in_target_key_block {
            target_key_base_indent = indent;

            if trimmed.contains("default:") {
                // Inline format: `key: { ..., default: "val", ... }` — replace on this line
                result.push(replace_default_inline(line, new_value));
                done = true;
            } else {
                // Block format: `key:\n  default: val` — enter this key's sub-block
                in_target_key_block = true;
                result.push(line.to_string());
            }
            continue;
        }

        if in_target_key_block {
            if indent > target_key_base_indent {
                // Still inside the key's block
                if trimmed.starts_with("default:") {
                    let space = " ".repeat(indent);
                    result.push(format!("{}default: {}", space, new_value));
                    done = true;
                    in_target_key_block = false;
                } else {
                    result.push(line.to_string());
                }
            } else {
                // Left the key's block without finding `default:` — pass through
                in_target_key_block = false;
                result.push(line.to_string());
            }
            continue;
        }

        result.push(line.to_string());
    }

    let mut output = result.join("\n");
    if had_trailing_newline && !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Replace the `default:` value inside an inline YAML dict on a single line.
/// e.g. `  key: { type: "string", default: "old", desc: "..." }`
///   → `  key: { type: "string", default: "new", desc: "..." }`
fn replace_default_inline(line: &str, new_value: &str) -> String {
    let Some(def_pos) = line.find("default:") else {
        return line.to_string();
    };

    let before = &line[..def_pos + 8]; // includes "default:"
    let after_colon = &line[def_pos + 8..];

    // Skip spaces between "default:" and the value
    let space_len = after_colon.len() - after_colon.trim_start_matches(' ').len();
    let spaces = &after_colon[..space_len];
    let value_start = &after_colon[space_len..];

    // Determine end of old value and whether it was quoted
    let (val_len, was_quoted) = if value_start.starts_with('"') {
        let end = value_start[1..].find('"').map(|p| p + 2).unwrap_or(value_start.len());
        (end, true)
    } else {
        // Unquoted: ends at ',' or '}' or end of trimmed line
        let end = value_start.find(|c: char| c == ',' || c == '}')
            .unwrap_or_else(|| value_start.trim_end().len());
        (end, false)
    };

    let after_value = &value_start[val_len..];

    // Use quotes if original was quoted OR if new value needs it
    let new_str = if was_quoted || new_value.is_empty()
        || new_value.contains(|c: char| c == ',' || c == '}' || c == ' ' || c == ':')
    {
        format!("\"{}\"", new_value)
    } else {
        new_value.to_string()
    };

    format!("{}{}{}{}", before, spaces, new_str, after_value)
}
