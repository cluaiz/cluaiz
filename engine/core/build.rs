//! The Engine's version has one home: `version` in the repository's root `package.json`.
//! Everything that shows a version (`cluaiz --version`, `system.info`, `/health`, the update check,
//! user agents) reads `cluaiz_core::VERSION`, which comes from here. Crate versions in Cargo.toml
//! are not the Engine's version.

use std::path::Path;

fn main() {
    let manifest = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../package.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-env-changed=CLUAIZ_RELEASE_VERSION");
    let version = std::fs::read_to_string(&manifest)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json["version"].as_str().map(str::to_owned))
        .unwrap_or_else(|| {
            println!("cargo:warning=No version in {}; using the crate version", manifest.display());
            std::env::var("CARGO_PKG_VERSION").unwrap()
        });
    println!("cargo:rustc-env=CLUAIZ_VERSION={version}");
}
