//! The Engine's version has one home: the root Cargo.toml, `[workspace.package] version` and
//! `[workspace.metadata.cluaiz] channel` ("pre-release" or "stable"). Everything that shows a
//! version (`cluaiz --version`, `system.info`, `/health`, the update check, user agents) reads
//! `cluaiz_core::VERSION` and `cluaiz_core::CHANNEL`, which come from here.

use std::path::Path;

fn main() {
    let manifest = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-env-changed=CLUAIZ_RELEASE_VERSION");

    // The release workflow sets the release's version and checks it equals Cargo.toml
    let version = std::env::var("CLUAIZ_RELEASE_VERSION")
        .ok()
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap());
    let channel = channel(&std::fs::read_to_string(&manifest).unwrap_or_default());
    let shown = if channel.is_empty() || channel == "stable" { version.clone() } else { format!("{version} ({channel})") };

    println!("cargo:rustc-env=CLUAIZ_VERSION={version}");
    println!("cargo:rustc-env=CLUAIZ_CHANNEL={channel}");
    println!("cargo:rustc-env=CLUAIZ_VERSION_SHOWN={shown}");
}

/// `channel = "..."` in the `[workspace.metadata.cluaiz]` table
fn channel(cargo_toml: &str) -> String {
    let mut inside = false;
    for line in cargo_toml.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == "[workspace.metadata.cluaiz]";
        } else if let Some(value) = line.strip_prefix("channel").filter(|_| inside) {
            return value.trim_start().trim_start_matches('=').trim().trim_matches('"').to_owned();
        }
    }
    String::new()
}
