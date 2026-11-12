//! Flow text (JSON or YAML 1.2) to one JSON model (docs/automation/file.md#formats).

use crate::problem::Problem;
use serde::Serialize;
use serde_json::Value;

/// At most this much text per flow (*proposed* 1 MB)
pub const MAX_TEXT_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Json,
    Yaml,
}

impl Format {
    pub fn parse(name: &str) -> Option<Format> {
        match name {
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Yaml => "yaml",
        }
    }

    /// The file suffix a flow of this format is saved with
    pub fn suffix(self) -> &'static str {
        match self {
            Format::Json => ".flow.json",
            Format::Yaml => ".flow.yaml",
        }
    }

    /// The format and name of a flow file, from its file name
    pub fn of_file(file_name: &str) -> Option<(Format, &str)> {
        [(".flow.json", Format::Json), (".flow.yaml", Format::Yaml), (".flow.yml", Format::Yaml)]
            .iter()
            .find_map(|(suffix, format)| file_name.strip_suffix(suffix).map(|name| (*format, name)))
    }
}

/// A problem is the natural error here: it carries the line and column
#[allow(clippy::result_large_err)]
pub fn parse_text(text: &str, format: Format) -> Result<Value, Problem> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(Problem::error("too_large", "", format!("the text is {} bytes; a flow is at most 1 MB", text.len())));
    }
    match format {
        Format::Json => serde_json::from_str(text)
            .map_err(|e| Problem::error("parse_error", "", format!("not valid JSON: {e}")).at(e.line(), e.column())),
        Format::Yaml => {
            let mut yaml: serde_yaml::Value = serde_yaml::from_str(text).map_err(|e| {
                let problem = Problem::error("parse_error", "", format!("not valid YAML: {e}"));
                match e.location() {
                    Some(at) => problem.at(at.line(), at.column()),
                    None => problem,
                }
            })?;
            if let Some(tag) = first_tag(&yaml) {
                return Err(Problem::error("parse_error", "", format!("YAML tags are not allowed (`{tag}`)")));
            }
            yaml.apply_merge().map_err(|e| Problem::error("parse_error", "", format!("a YAML merge (<<) failed: {e}")))?;
            serde_json::to_value(yaml).map_err(|e| Problem::error("parse_error", "", format!("the YAML has a value JSON cannot hold: {e}")))
        }
    }
}

fn first_tag(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::Tagged(tagged) => Some(tagged.tag.to_string()),
        serde_yaml::Value::Sequence(items) => items.iter().find_map(first_tag),
        serde_yaml::Value::Mapping(map) => map.iter().find_map(|(k, v)| first_tag(k).or_else(|| first_tag(v))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn yaml_is_read_as_1_2() {
        let value = parse_text("on: [ { manual: {} } ]\nanswer: yes\nflag: true\n", Format::Yaml).unwrap();
        assert_eq!(value, json!({ "on": [{ "manual": {} }], "answer": "yes", "flag": true }));
    }

    #[test]
    fn errors_have_a_place() {
        let problem = parse_text("{\n  \"a\": 1,\n  \"b\" 2\n}", Format::Json).unwrap_err();
        assert_eq!((problem.code, problem.line), ("parse_error", Some(3)));
        let problem = parse_text("a: !secret x\n", Format::Yaml).unwrap_err();
        assert!(problem.message.contains("tags are not allowed"));
        assert_eq!(parse_text(&" ".repeat(MAX_TEXT_BYTES + 1), Format::Json).unwrap_err().code, "too_large");
    }

    #[test]
    fn file_names() {
        assert_eq!(Format::of_file("news.flow.yml"), Some((Format::Yaml, "news")));
        assert_eq!(Format::of_file("news.yaml"), None);
    }
}
