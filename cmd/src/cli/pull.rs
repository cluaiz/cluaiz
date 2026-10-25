//! `cluaiz pull <id>`: download and register a model (the same flow `cluaiz run` uses for a model that is
//! not installed). It ends with the command to run the model.

use color_eyre::Result;
use colored::Colorize;
use crate::cli::install::{self, Resolved};

pub async fn execute(model_id: &str) -> Result<()> {
    let plan = match install::resolve(model_id, false).await? {
        Resolved::OnDisk(path) => {
            let id = cluaiz_models::registry::InstalledStateRegistry::id_for_path(&path);
            println!("\n  {} Model '{}' is already downloaded.", "⚠️".yellow(), id.cyan());
            println!("     If you wish to re-download, please delete the old one first using: cluaiz rm {}\n", id.red());
            return Ok(());
        }
        Resolved::AlreadyInstalled(id) => {
            println!("\n  {} Warning: This exact variant is already downloaded locally under ID: '{}'", "⚠️".yellow(), id.cyan());
            println!("     If you wish to re-download, please delete the old one first using: cluaiz rm {}\n", id.red());
            return Ok(());
        }
        Resolved::Download(plan) => plan,
    };
    install::confirm(&plan)?;
    install::download(&plan).await?;
    Ok(())
}
