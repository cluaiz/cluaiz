//! `cluaiz self update` and `cluaiz self uninstall`: the installed Engine itself (as `rustup self`).
//!
//! An update runs the release's own installer (`install.sh` / `install.ps1` from that release's tag),
//! so it picks the GPU pack and checks every file exactly as a first install does; models and
//! settings stay. An uninstall deletes the Cluaiz home (`~/.cluaiz`, or `CLUAIZ_HOME`), the program
//! folder and the PATH entry the installer added. The Cluaiz App is a separate program.

use clap::Subcommand;
use cluaiz_api::update::{self, REPO};
use color_eyre::{eyre::eyre, Result};
use colored::Colorize;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum SelfCommand {
    /// Update Cluaiz to the newest release (models and settings stay)
    Update {
        /// Only say whether a newer release is out
        #[arg(long)]
        check: bool,
        /// Install this release instead of the newest one (e.g. 0.1.2; an older one too)
        #[arg(long, value_name = "VERSION")]
        to: Option<String>,
    },
    /// Remove Cluaiz: its folder, its models (unless --keep-models) and its PATH entry
    Uninstall {
        /// Keep the downloaded models and their list, for a later install
        #[arg(long)]
        keep_models: bool,
        /// Do not ask
        #[arg(short, long)]
        yes: bool,
    },
}

pub async fn execute(command: SelfCommand) -> Result<()> {
    match command {
        SelfCommand::Update { check, to } => self_update(check, to).await,
        SelfCommand::Uninstall { keep_models, yes } => uninstall(keep_models, yes).await,
    }
}

// ── Update ─────────────────────────────────────────────────────────────────────────────────────

async fn self_update(check: bool, to: Option<String>) -> Result<()> {
    let current = cluaiz_core::VERSION;
    let target = match to {
        Some(version) => version.trim().trim_start_matches('v').to_string(),
        None => {
            let latest = update::latest_release().await.map_err(|error| eyre!(error))?;
            if !update::is_newer(&latest.version, current) {
                println!("  {} Cluaiz {current} is up to date (newest release: {}).", "✓".green(), latest.version);
                return Ok(());
            }
            println!("  Cluaiz {} is out (this is {current}): {}", latest.version.bold(), latest.url);
            latest.version
        }
    };
    if check {
        println!("  Run `cluaiz self update` to install it.");
        return Ok(());
    }
    if target.is_empty() {
        return Err(eyre!("--to needs a version, e.g. --to 0.1.2"));
    }
    let dir = install_dir()?;
    refuse_running_server().await?;
    run_installer(&target, &dir).await
}

/// Downloads the installer of release `version` and runs it into `dir`
async fn run_installer(version: &str, dir: &Path) -> Result<()> {
    let name = if cfg!(windows) { "install.ps1" } else { "install.sh" };
    let url = format!("https://raw.githubusercontent.com/{REPO}/v{version}/{name}");
    let response = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|error| eyre!("the installer could not be downloaded from {url}: {error}"))?;
    if !response.status().is_success() {
        return Err(eyre!("no installer for release {version} ({url}: HTTP {}); is the version right?", response.status()));
    }
    let script = std::env::temp_dir().join(format!("cluaiz-update-{}-{name}", std::process::id()));
    std::fs::write(&script, response.bytes().await?)?;
    println!("  Updating {} to {} with the release's installer...", dir.display(), version.bold());

    #[cfg(windows)]
    {
        // This cluaiz.exe and its DLLs stay locked while it runs: the installer starts in its own
        // window once this command has exited
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let wait = format!(
            "Wait-Process -Id {} -ErrorAction SilentlyContinue; try {{ & $env:CLUAIZ_UPDATE_SCRIPT }} catch {{ }} \
             finally {{ Remove-Item -LiteralPath $env:CLUAIZ_UPDATE_SCRIPT -ErrorAction SilentlyContinue; Read-Host 'Press Enter to close' }}",
            std::process::id()
        );
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &wait])
            .env("CLUAIZ_UPDATE_SCRIPT", &script)
            .env("CLUAIZ_VERSION", version)
            .env("CLUAIZ_INSTALL_DIR", dir)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|error| eyre!("PowerShell could not start the installer: {error}"))?;
        println!("  The installer continues in a new window. Open a new terminal afterwards.");
        Ok(())
    }
    #[cfg(not(windows))]
    {
        // A running binary can be replaced on Linux and macOS: the installer runs here
        let status = std::process::Command::new("sh")
            .arg(&script)
            .env("CLUAIZ_VERSION", version)
            .env("CLUAIZ_INSTALL_DIR", dir)
            .status();
        let _ = std::fs::remove_file(&script);
        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(eyre!("the installer failed ({status}); the files that were there stay in use")),
            Err(error) => Err(eyre!("sh could not run the installer: {error}")),
        }
    }
}

// ── Uninstall ──────────────────────────────────────────────────────────────────────────────────

async fn uninstall(keep_models: bool, yes: bool) -> Result<()> {
    let bin = install_dir()?;
    let env = cluaiz_core::EnvironmentManager::current();
    let home = env.global_dir.clone();
    let models = env.models_dir();
    refuse_running_server().await?;

    let models_size = folder_size(&models);
    println!("\n  {}", "Cluaiz will be removed:".bold());
    println!("    {}  ({})", home.display(), gb(folder_size(&home)));
    if keep_models {
        println!("      {} the models ({}, {}) and their list", "except".green(), gb(models_size), models.display());
    } else if models_size > 0 {
        println!("      {} the downloaded models ({}); --keep-models keeps them", "including".red(), gb(models_size));
    }
    if !bin.starts_with(&home) {
        println!("    {}  (the program)", bin.display());
    }
    println!("    the PATH entry for {}", bin.display());
    println!("  {}", "The Cluaiz App is a separate program; it is not removed.".dimmed());

    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err(eyre!("no terminal to ask: run `cluaiz self uninstall --yes`"));
        }
        let go = inquire::Confirm::new("Remove Cluaiz?").with_default(false).prompt().unwrap_or(false);
        if !go {
            println!("  Nothing was removed.");
            return Ok(());
        }
    }

    match remove_path_entry(&bin) {
        Ok(true) => println!("  {} PATH entry removed", "✓".green()),
        Ok(false) => {}
        Err(error) => println!("  {} PATH entry: {error}; remove {} from PATH yourself", "!".yellow(), bin.display()),
    }

    // Everything in the home, the kept models aside; the program folder last
    let mut left = Vec::new();
    for entry in std::fs::read_dir(&home).into_iter().flatten().flatten() {
        let path = entry.path();
        if path == bin || (keep_models && path == models) {
            continue;
        }
        if keep_models && path == env.config_dir() {
            for file in std::fs::read_dir(&path).into_iter().flatten().flatten() {
                if file.file_name() != "model_registry.json" && remove(&file.path()).is_err() {
                    left.push(file.path());
                }
            }
            continue;
        }
        if remove(&path).is_err() {
            left.push(path);
        }
    }
    finish_removal(&bin, &home, !keep_models, &left)?;
    println!("  {} Cluaiz is removed. Thank you for trying it.", "✓".green());
    // Anything that ran after this (settings, logs) would create the home again
    std::process::exit(0);
}

/// Deletes the program folder (and the home when it is left empty). On Windows this cluaiz.exe and
/// its DLLs are locked while it runs: a hidden PowerShell deletes them once it has exited, with
/// whatever else was in use.
fn finish_removal(bin: &Path, home: &Path, remove_home: bool, left: &[PathBuf]) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let left = left.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("|");
        let script = format!(
            "Wait-Process -Id {} -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 500; \
             foreach ($p in @($env:CLUAIZ_REMOVE_BIN) + @($env:CLUAIZ_REMOVE_LEFT -split '\\|' | Where-Object {{ $_ }})) {{ \
               Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction SilentlyContinue }}; \
             if ($env:CLUAIZ_REMOVE_HOME -eq '1' -and (Test-Path -LiteralPath $env:CLUAIZ_HOME_DIR) -and \
                 -not (Get-ChildItem -LiteralPath $env:CLUAIZ_HOME_DIR -Force)) {{ Remove-Item -LiteralPath $env:CLUAIZ_HOME_DIR -Force }}",
            std::process::id()
        );
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
            .env("CLUAIZ_REMOVE_BIN", bin)
            .env("CLUAIZ_REMOVE_LEFT", left)
            .env("CLUAIZ_REMOVE_HOME", if remove_home { "1" } else { "0" })
            .env("CLUAIZ_HOME_DIR", home)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| eyre!("the program folder {} could not be scheduled for removal: {error}", bin.display()))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let mut failed: Vec<PathBuf> = Vec::new();
        for path in left.iter().map(PathBuf::as_path).chain([bin]) {
            if path.exists() && remove(path).is_err() {
                failed.push(path.to_path_buf());
            }
        }
        if remove_home && std::fs::read_dir(home).map(|mut d| d.next().is_none()).unwrap_or(false) {
            let _ = std::fs::remove_dir(home);
        }
        if failed.is_empty() {
            Ok(())
        } else {
            let list = failed.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
            Err(eyre!("these could not be removed: {list}"))
        }
    }
}

fn remove(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Removes the installer's PATH entry for `bin`: true when one was there
fn remove_path_entry(bin: &Path) -> Result<bool> {
    #[cfg(windows)]
    {
        // The user PATH (where install.ps1 added it), same comparison as Windows: no case, no trailing \
        let script = "$d = $env:CLUAIZ_REMOVE_DIR.TrimEnd('\\'); $p = [Environment]::GetEnvironmentVariable('Path', 'User'); \
                      $all = @(([string]$p) -split ';' | Where-Object { $_ }); \
                      $kept = @($all | Where-Object { $_.TrimEnd('\\') -ne $d }); \
                      if ($kept.Count -ne $all.Count) { [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User'); 'removed' }";
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
            .env("CLUAIZ_REMOVE_DIR", bin)
            .output()
            .map_err(|error| eyre!("PowerShell could not start: {error}"))?;
        if !output.status.success() {
            return Err(eyre!("{}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).contains("removed"))
    }
    #[cfg(not(windows))]
    {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return Ok(false) };
        let mut removed = false;
        for rc in [".bashrc", ".zshrc", ".profile"] {
            let rc = home.join(rc);
            let Ok(text) = std::fs::read_to_string(&rc) else { continue };
            if let Some(stripped) = strip_path_block(&text, &bin.display().to_string()) {
                std::fs::write(&rc, stripped)?;
                removed = true;
            }
        }
        Ok(removed)
    }
}

/// `text` without the block install.sh appends (an empty line, `# Cluaiz`, the export line), or
/// None when that export line is not there. Only the installer's exact line is removed.
#[cfg_attr(windows, allow(dead_code))]
fn strip_path_block(text: &str, bin: &str) -> Option<String> {
    let export = format!("export PATH=\"{bin}:$PATH\"");
    let mut lines: Vec<&str> = text.split('\n').collect();
    let at = lines.iter().position(|line| line.trim_end() == export)?;
    let mut start = at;
    if start > 0 && lines[start - 1].trim_end() == "# Cluaiz" {
        start -= 1;
        if start > 0 && lines[start - 1].trim().is_empty() {
            start -= 1;
        }
    }
    lines.drain(start..=at);
    Some(lines.join("\n"))
}

// ── Shared ─────────────────────────────────────────────────────────────────────────────────────

/// The folder this cluaiz runs from; a development build (inside a Cargo workspace) is refused,
/// so `cargo build` output and the source tree are never replaced or deleted
fn install_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| eyre!("no folder for {}", exe.display()))?.to_path_buf();
    if dir.ancestors().any(|folder| folder.join("Cargo.toml").is_file()) {
        return Err(eyre!(
            "this cluaiz is a development build ({}); update it with git pull and cargo build, or run the installed one",
            exe.display()
        ));
    }
    Ok(dir)
}

/// A server running from the install folder keeps its files open: it has to stop first. It is not
/// stopped from here (it can be the App's).
async fn refuse_running_server() -> Result<()> {
    if let Ok(live) = crate::cli::server::cep("system.live", serde_json::json!({})).await {
        let pid = live["process_id"].as_u64().map(|pid| format!(" (PID {pid})")).unwrap_or_default();
        return Err(eyre!(
            "a Cluaiz server is running{pid}: close the Cluaiz App or stop `cluaiz serve`, then run this again"
        ));
    }
    Ok(())
}

fn folder_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| folder_size(&entry.path()))
        .sum()
}

fn gb(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.0} MB", bytes as f64 / (1u64 << 20) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_a_real_newer_release_counts() {
        assert!(update::is_newer("0.2.0", "0.1.0"));
        assert!(update::is_newer("v0.1.1", "0.1.0"));
        assert!(update::is_newer("0.1.10", "0.1.9"));
        assert!(update::is_newer("0.2.0", "0.2.0-rc1"));
        assert!(!update::is_newer("0.2.0-rc1", "0.2.0"));
        assert!(!update::is_newer("0.1.0", "0.1.0"));
        assert!(!update::is_newer("0.0.9", "0.1.0"));
        assert!(!update::is_newer("dev-release", "0.1.0"));
    }

    #[test]
    fn test_newest_release_is_read_from_the_github_list() {
        let body = r#"[{"tag_name":"v0.1.2","html_url":"https://github.com/cluaiz/cluaiz/releases/tag/v0.1.2",
                        "prerelease":true,"published_at":"2026-11-01T08:00:00Z"},{"tag_name":"v0.1.1"}]"#;
        let release = update::parse_releases(body).unwrap();
        assert_eq!(release.version, "0.1.2");
        assert_eq!(release.tag, "v0.1.2");
        assert!(release.prerelease);
        assert!(update::parse_releases("[]").is_err());
    }

    #[test]
    fn test_uninstall_removes_only_the_installer_path_block() {
        let bin = "/home/a/.cluaiz/bin";
        let rc = "alias ll='ls -l'\n\n# Cluaiz\nexport PATH=\"/home/a/.cluaiz/bin:$PATH\"\n";
        assert_eq!(strip_path_block(rc, bin).unwrap(), "alias ll='ls -l'\n");
        // A line the user wrote differently stays
        let own = "export PATH=\"$PATH:/home/a/.cluaiz/bin\"\n";
        assert!(strip_path_block(own, bin).is_none());
        // Without the comment only the export line goes
        let bare = "a\nexport PATH=\"/home/a/.cluaiz/bin:$PATH\"\nb\n";
        assert_eq!(strip_path_block(bare, bin).unwrap(), "a\nb\n");
    }
}
