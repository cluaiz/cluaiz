//! Every complete flow in docs/automation is checked by the validator, so the docs and the code
//! cannot drift apart: a flow the docs show must have no errors.

use cluaiz_automation::format::{parse_text, Format};
use cluaiz_automation::host::NoCatalog;
use cluaiz_automation::validate::validate;
use std::path::{Path, PathBuf};

fn pages(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            pages(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// Code blocks of a page: (language, text)
fn blocks(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in text.lines() {
        match (&mut current, line.strip_prefix("```")) {
            (None, Some(language)) => current = Some((language.trim().to_string(), String::new())),
            (Some(_), Some(_)) => out.extend(current.take()),
            (Some((_, body)), None) => {
                body.push_str(line);
                body.push('\n');
            }
            (None, None) => {}
        }
    }
    out
}

#[test]
fn every_complete_flow_in_the_docs_has_no_errors() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/automation");
    let mut files = Vec::new();
    pages(&docs, &mut files);
    files.push(docs.join("../reference/automation.md"));
    let mut checked = 0;
    let mut failures = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        for (language, body) in blocks(&text) {
            let format = match language.as_str() {
                "yaml" => Format::Yaml,
                "json" => Format::Json,
                _ => continue,
            };
            if !body.contains("cluaiz.flow/1") || !body.contains("steps") {
                continue;
            }
            let Ok(flow) = parse_text(&body, format) else { continue };
            // Only whole flows: a fragment that shows one field is not a flow
            if flow.get("flow").is_none() || flow.get("name").is_none() {
                continue;
            }
            checked += 1;
            let report = validate(&flow, None, &NoCatalog);
            let errors: Vec<String> = report.problems.iter().filter(|p| p.is_error()).map(|p| format!("{} {}: {}", p.code, p.path, p.message)).collect();
            if !errors.is_empty() {
                failures.push(format!("{} ({}):\n  {}", file.display(), flow["name"], errors.join("\n  ")));
            }
        }
    }
    assert!(checked >= 10, "only {checked} flows found in the docs");
    assert!(failures.is_empty(), "{} of {checked} flows in the docs have errors:\n{}", failures.len(), failures.join("\n"));
}
