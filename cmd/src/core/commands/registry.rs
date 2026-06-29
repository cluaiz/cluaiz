use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct CommandMetadata {
    pub name: String,
    pub usage: String,
    pub description: String,
    pub category: String,
    pub example: String,
}

#[derive(Debug, Deserialize)]
pub struct CommandRegistry {
    pub version: String,
    pub commands: Vec<CommandMetadata>,
}

impl CommandRegistry {
    /// 📂 Industrial Load: Pulls command truth from the local assets.
    pub fn load() -> Result<Self> {
        let content = include_str!("../../../assets/commands.json");
        let registry: CommandRegistry = serde_json::from_str(content)?;
        Ok(registry)
    }

    /// 🏛️ Help Generator: Dynamically builds the help screen from the JSON registry.
    pub fn generate_help(&self) {
        use colored::Colorize;

        println!(
            "\n  {} cluaiz Engine CLI v{}",
            "🚀".magenta(),
            self.version.bold()
        );
        println!("  Source: {}\n", "commands.json".cyan());

        let mut categories: Vec<String> =
            self.commands.iter().map(|c| c.category.clone()).collect();
        categories.sort();
        categories.dedup();

        for cat in categories {
            println!("  {}", cat.to_uppercase().bold().yellow());
            for cmd in self.commands.iter().filter(|c| c.category == cat) {
                println!(
                    "    {:<12} {}",
                    cmd.name.green().bold(),
                    cmd.description.dimmed()
                );
                println!("    {} {}\n", "Usage:".dimmed(), cmd.usage.italic());
            }
        }

        println!(
            "  Use {} to launch the neural cockpit.\n",
            "cluaiz".bold().magenta()
        );
    }
}
