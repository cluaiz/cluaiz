use color_eyre::Result;
use colored::Colorize;
use serde::Deserialize;

// ── Schema ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct CommandEntry {
    pub name: String,
    pub usage: String,
    pub description: String,
    pub category: String,
    pub example: String,
}

#[derive(Debug, Deserialize)]
struct CommandRegistry {
    pub commands: Vec<CommandEntry>,
}

// ── Compile-time embed ──────────────────────────────────────────────────────
// commands.json is baked into the binary at compile time.
// To add a new command: edit assets/commands.json and recompile.
const COMMANDS_JSON: &str = include_str!("../../assets/commands.json");

pub fn load_commands() -> Vec<CommandEntry> {
    serde_json::from_str::<CommandRegistry>(COMMANDS_JSON)
        .map(|r| r.commands)
        .unwrap_or_else(|_| embedded_defaults())
}

/// Hard-coded fallback — used only if JSON is somehow malformed at build time.
fn embedded_defaults() -> Vec<CommandEntry> {
    vec![
        CommandEntry {
            name: "cluaiz".into(),
            usage: "cluaiz".into(),
            description: "Launch the interactive dashboard (TUI)".into(),
            category: "core".into(),
            example: "cluaiz".into(),
        },
        CommandEntry {
            name: "run".into(),
            usage: "cluaiz run <model-id>".into(),
            description: "Pull & run a model. Downloads if not cached locally.".into(),
            category: "models".into(),
            example: "cluaiz run bonsai:8b".into(),
        },
        CommandEntry {
            name: "help".into(),
            usage: "cluaiz help".into(),
            description: "Show this help screen".into(),
            category: "core".into(),
            example: "cluaiz help".into(),
        },
        CommandEntry {
            name: "--calibrate".into(),
            usage: "cluaiz --calibrate".into(),
            description: "Re-scan hardware and synchronize SiliconTruth profile".into(),
            category: "system".into(),
            example: "cluaiz --calibrate".into(),
        },
        CommandEntry {
            name: "--benchmark".into(),
            usage: "cluaiz --benchmark".into(),
            description: "Run a full hardware performance benchmark".into(),
            category: "system".into(),
            example: "cluaiz --benchmark".into(),
        },
    ]
}

// ── Terminal Help Printer ───────────────────────────────────────────────────

pub fn print_help() -> Result<()> {
    let commands = load_commands();

    println!();
    println!(
        "  {} {}",
        "🧬".cyan(),
        "cluaiz CLI — Universal Neural Kernel".bold()
    );
    println!();
    println!("  {}  cluaiz [COMMAND]", "USAGE:".bold());
    println!();

    let categories: &[(&str, &str)] = &[
        ("core",   "Core"),
        ("models", "Models"),
        ("system", "System"),
    ];

    for (cat_key, cat_label) in categories {
        let group: Vec<&CommandEntry> = commands
            .iter()
            .filter(|c| c.category.as_str() == *cat_key)
            .collect();

        if group.is_empty() {
            continue;
        }

        println!("  {}:", cat_label.bold().underline());

        for cmd in group {
            let usage_col = format!("  {}", cmd.usage);
            println!(
                "  {:<42}  {}",
                usage_col.green().bold(),
                cmd.description.dimmed()
            );
        }
        println!();
    }

    println!(
        "  {} Edit {} to add commands — baked into binary at compile time.",
        "💡".cyan(),
        "assets/commands.json".yellow()
    );
    println!();

    Ok(())
}
