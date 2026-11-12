//! References: paths into the run's context (`steps.a.output.items[0]`), and `${{ path }}` inside
//! strings (docs/automation/references.md). A reference holds a path only, never a calculation.

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Field(String),
    Index(i64),
}

/// `a.b`, `a[2]`, `a[-1]`, `a["key with spaces"]`
pub fn parse_path(path: &str) -> Result<Vec<Segment>, String> {
    let bad = |why: &str| format!("`{path}` is not a path: {why}");
    let mut segments = Vec::new();
    let chars: Vec<char> = path.trim().chars().collect();
    let mut i = 0;
    let mut expect_name = true;
    while i < chars.len() {
        match chars[i] {
            '.' => {
                if expect_name {
                    return Err(bad("a name is missing before `.`"));
                }
                expect_name = true;
                i += 1;
            }
            '[' => {
                let end = (i + 1..chars.len()).find(|&j| chars[j] == ']' && !in_quotes(&chars[i + 1..j])).ok_or_else(|| bad("`[` without `]`"))?;
                let inner: String = chars[i + 1..end].iter().collect();
                let inner = inner.trim();
                if let Some(quoted) = inner.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                    segments.push(Segment::Field(quoted.replace("\\\"", "\"")));
                } else {
                    let index: i64 = inner.parse().map_err(|_| bad("an index must be a whole number or quoted text"))?;
                    segments.push(Segment::Index(index));
                }
                expect_name = false;
                i = end + 1;
            }
            _ => {
                if !expect_name {
                    return Err(bad("expected `.` or `[`"));
                }
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-') {
                    i += 1;
                }
                if start == i {
                    return Err(bad(&format!("`{}` is not allowed here", chars[i])));
                }
                segments.push(Segment::Field(chars[start..i].iter().collect()));
                expect_name = false;
            }
        }
    }
    if segments.is_empty() || expect_name {
        return Err(bad("it ends too early"));
    }
    Ok(segments)
}

fn in_quotes(chars: &[char]) -> bool {
    chars.iter().filter(|&&c| c == '"').count() % 2 == 1
}

/// The value at the path, or `null` when any part is missing
pub fn lookup(root: &Value, segments: &[Segment]) -> Value {
    let mut current = root;
    for segment in segments {
        let next = match (segment, current) {
            (Segment::Field(name), Value::Object(map)) => map.get(name),
            (Segment::Index(index), Value::Array(items)) => {
                let at = if *index < 0 { items.len() as i64 + index } else { *index };
                usize::try_from(at).ok().and_then(|at| items.get(at))
            }
            _ => None,
        };
        match next {
            Some(value) => current = value,
            None => return Value::Null,
        }
    }
    current.clone()
}

/// One piece of a string: plain text or a reference
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Ref(String),
}

/// Splits a string into text and `${{ path }}` references; `$${{` is the text `${{`
pub fn split_template(text: &str) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("${{") {
        if at > 0 && rest[..at].ends_with('$') {
            plain.push_str(&rest[..at - 1]);
            plain.push_str("${{");
            rest = &rest[at + 3..];
            continue;
        }
        plain.push_str(&rest[..at]);
        let after = &rest[at + 3..];
        let end = after.find("}}").ok_or_else(|| format!("`${{{{` without `}}}}` in \"{text}\""))?;
        if !plain.is_empty() {
            pieces.push(Piece::Text(std::mem::take(&mut plain)));
        }
        pieces.push(Piece::Ref(after[..end].trim().to_string()));
        rest = &after[end + 2..];
    }
    plain.push_str(rest);
    if !plain.is_empty() {
        pieces.push(Piece::Text(plain));
    }
    Ok(pieces)
}

/// The paths of every reference in a value (strings, and inside lists and objects)
pub fn references_in(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if let Ok(pieces) = split_template(text) {
                out.extend(pieces.into_iter().filter_map(|p| match p {
                    Piece::Ref(path) => Some(path),
                    Piece::Text(_) => None,
                }));
            }
        }
        Value::Array(items) => items.iter().for_each(|v| references_in(v, out)),
        Value::Object(map) => map.values().for_each(|v| references_in(v, out)),
        _ => {}
    }
}

/// A value inside other text: numbers in their shortest form, `null` as nothing, lists and
/// objects as compact JSON, a file reference as its path
pub fn to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Object(map) if map.get("$file").is_some_and(Value::is_string) => {
            map["$file"].as_str().unwrap_or_default().to_string()
        }
        other => other.to_string(),
    }
}

/// Is this object a file reference (`{ "$file": … }`)?
pub fn is_file_ref(value: &Value) -> bool {
    value.as_object().is_some_and(|m| m.get("$file").is_some_and(Value::is_string))
}

/// Builds `{ "$file": path, "bytes": n, "media_type": type }`
pub fn file_ref(path: &str, bytes: u64, media_type: &str) -> Value {
    let mut map = Map::new();
    map.insert("$file".into(), Value::String(path.to_string()));
    map.insert("bytes".into(), Value::from(bytes));
    map.insert("media_type".into(), Value::String(media_type.to_string()));
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths() {
        assert_eq!(
            parse_path(r#"trigger.headers["x-shop-id"]"#).unwrap(),
            vec![Segment::Field("trigger".into()), Segment::Field("headers".into()), Segment::Field("x-shop-id".into())]
        );
        assert_eq!(parse_path("steps.rows.output[-1]").unwrap().last(), Some(&Segment::Index(-1)));
        for bad in ["", "a.", ".a", "a..b", "a[", "a[x]", "a b"] {
            assert!(parse_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn lookups_give_null_when_missing() {
        let root = json!({ "a": { "b": [1, 2, 3] } });
        let get = |p: &str| lookup(&root, &parse_path(p).unwrap());
        assert_eq!(get("a.b[0]"), json!(1));
        assert_eq!(get("a.b[-1]"), json!(3));
        assert_eq!(get("a.b[9]"), Value::Null);
        assert_eq!(get("a.c.d"), Value::Null);
        assert_eq!(get("a.b.c"), Value::Null);
    }

    #[test]
    fn templates() {
        assert_eq!(
            split_template("Hi ${{ inputs.name }}!").unwrap(),
            vec![Piece::Text("Hi ".into()), Piece::Ref("inputs.name".into()), Piece::Text("!".into())]
        );
        assert_eq!(split_template("cost $${{ x }}").unwrap(), vec![Piece::Text("cost ${{ x }}".into())]);
        assert!(split_template("${{ open").is_err());
        assert_eq!(to_text(&json!(2.5)), "2.5");
        assert_eq!(to_text(&Value::Null), "");
        assert_eq!(to_text(&json!([1, "a"])), r#"[1,"a"]"#);
    }
}
