use color_eyre::Result;
use colored::Colorize;
use engines::models::registry::CoreRoster;

pub async fn execute() -> Result<()> {
    println!("\n  {} [cluaiz] Scanning Vault for Neural Weights...\n", "🔍".cyan());

    let roster = CoreRoster::load_roster();
    
    if roster.is_empty() {
        println!("     {} No models found in the vault.", "⚠️".yellow());
        println!("     {} Use 'cluaiz run <id>' to download your first model.\n", "💡".cyan());
        return Ok(());
    }

    println!("  {:<20} {:<15} {:<10} {:<10}", "ID".bold(), "NAME".bold(), "SIZE".bold(), "ARCH".bold());
    println!("  {}", "-".repeat(60).dimmed());

    for model in &roster {
        println!("  {:<20} {:<15} {:<10} {:<10}", 
            model.id.green(), 
            model.name, 
            format!("{:.1} GB", model.ram_required_gb).dimmed(),
            model.architecture.dimmed()
        );
    }

    println!("\n  {} Total models: {}\n", "📊".blue(), roster.len());

    Ok(())
}
