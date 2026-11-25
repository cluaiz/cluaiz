//! GBNF grammar text helpers for "a tool call OR JSON matching the response schema".
//!
//! llama.cpp gives two grammars for a request with `response_format` and tools, but never one that
//! allows both: with a JSON schema its parsers keep only the schema (common/chat-auto-parser-generator.cpp
//! `build_parser`: `if (has_response_format) ... else if (has_tools ...)`), so calls are impossible.
//! The Engine asks llama.cpp for both grammars (a lazy tool-call grammar from a render with tools, a
//! strict schema grammar from a render without them) and joins them here: the schema grammar's
//! `response-format` rule gets the tool-call grammar's root as a second alternative. The thinking
//! block, the optional ```json fence and the spaces around the answer stay as llama.cpp wrote them.
//!
//! Pure text work, no llama.cpp: rule names follow llama-grammar.cpp (`is_word_char`: ASCII letters,
//! digits and `-`; a definition is a name followed by `::=` on the same line). Strings `"..."`,
//! character classes `[...]`, tokens `<...>` / `!<...>`, repetition counts `{m,n}` and `#` comments
//! are copied unchanged.

/// The rule the schema branch must have: llama.cpp's autoparser names the answer rule this way
/// (`p.rule("response-format", ...)`)
pub const ANSWER_RULE: &str = "response-format";

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

/// Copies `gbnf`, passing every rule name (definitions and references) through `rename`, which also
/// learns whether the name is being defined. Errors on an unterminated string, class, token or count.
fn map_names(gbnf: &str, mut rename: impl FnMut(&str, bool) -> String) -> Result<String, String> {
    let chars: Vec<char> = gbnf.chars().collect();
    let mut out = String::with_capacity(gbnf.len() + 64);
    let mut i = 0;
    // Copies up to and including `close`, honouring backslash escapes when `escapes` is set
    let copy_until = |out: &mut String, i: &mut usize, close: char, escapes: bool, what: &str| -> Result<(), String> {
        out.push(chars[*i]);
        *i += 1;
        while *i < chars.len() {
            let c = chars[*i];
            out.push(c);
            *i += 1;
            if escapes && c == '\\' {
                if *i < chars.len() {
                    out.push(chars[*i]);
                    *i += 1;
                }
                continue;
            }
            if c == close {
                return Ok(());
            }
        }
        Err(format!("unterminated {what} in the grammar"))
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => copy_until(&mut out, &mut i, '"', true, "string")?,
            '[' => copy_until(&mut out, &mut i, ']', true, "character class")?,
            '<' => copy_until(&mut out, &mut i, '>', false, "token")?,
            '{' => copy_until(&mut out, &mut i, '}', false, "repetition count")?,
            '#' => {
                while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            c if is_word_char(c) => {
                let start = i;
                while i < chars.len() && is_word_char(chars[i]) {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();
                // A definition: spaces or tabs, then `::=` (llama-grammar.cpp `parse_rule`)
                let mut j = i;
                while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                    j += 1;
                }
                let defining = chars.get(j..j + 3).is_some_and(|s| s == [':', ':', '=']);
                out.push_str(&rename(&name, defining));
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(out)
}

/// The names the grammar defines, in order
pub fn rule_names(gbnf: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    map_names(gbnf, |name, defining| {
        if defining && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
        name.to_string()
    })?;
    Ok(names)
}

/// Every rule name with `prefix` in front, so two grammars can live in one text
pub fn prefix_rules(gbnf: &str, prefix: &str) -> Result<String, String> {
    if prefix.is_empty() || !prefix.chars().all(is_word_char) {
        return Err(format!("prefix {prefix:?} is not a rule name"));
    }
    map_names(gbnf, |name, _| format!("{prefix}{name}"))
}

/// Whether llama.cpp's lazy tool-call triggers can be joined with a JSON answer: every trigger a
/// plain word (a tag such as `<tool_call>`), none starting like JSON. A call format that starts
/// with `{` or `[` cannot be told apart from the answer by the parser, and pattern or token
/// triggers (gpt-oss channels) belong to formats whose answer is not bare JSON.
pub fn triggers_allow_merge(triggers: &serde_json::Value) -> bool {
    let Some(list) = triggers.as_array().filter(|list| !list.is_empty()) else {
        return false;
    };
    // JSON's own start: `{`, or `[` before a value (`[TOOL_CALLS]` is a tag, `[{` is JSON)
    let json_like = |value: &str| {
        value.starts_with('{')
            || value.strip_prefix('[').is_some_and(|rest| {
                rest.trim_start().chars().next().is_none_or(|c| matches!(c, '{' | '[' | '"' | ']' | '-') || c.is_ascii_digit())
            })
    };
    list.iter().all(|trigger| {
        let value = trigger["value"].as_str().unwrap_or_default().trim_start();
        trigger["type"].as_str() == Some("word") && !value.is_empty() && !json_like(value)
    })
}

/// A name built from `base` that `taken` does not hold (`base`, then `base1`, `base2`, ...)
fn free_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    (1..).map(|n| format!("{base}{n}")).find(|name| !taken(name)).unwrap_or_default()
}

/// One grammar from llama.cpp's strict schema grammar (`schema_gbnf`, with its `response-format`
/// rule) and its lazy tool-call grammar (`tool_gbnf`, whose `root` lists the call rules): where the
/// answer's JSON may start, a tool call may start instead.
///
/// The tool grammar's rules are prefixed (`call-`, or `call1-` ... when taken), the schema's
/// `response-format` rule becomes `response-format-schema-only` (or a free name), and a new
/// `response-format ::= response-format-schema-only | call-root` closes it. Applied strictly.
pub fn answer_or_tool_call(schema_gbnf: &str, tool_gbnf: &str) -> Result<String, String> {
    let schema_names = rule_names(schema_gbnf)?;
    let tool_names = rule_names(tool_gbnf)?;
    if !schema_names.iter().any(|n| n == "root") {
        return Err("the schema grammar has no root".into());
    }
    if !schema_names.iter().any(|n| n == ANSWER_RULE) {
        return Err(format!("the schema grammar has no `{ANSWER_RULE}` rule"));
    }
    if !tool_names.iter().any(|n| n == "root") {
        return Err("the tool-call grammar has no root".into());
    }
    let schema_taken = |name: &str| schema_names.iter().any(|n| n == name);
    // A prefix under which no tool rule meets a schema rule
    let prefix = (0..)
        .map(|n: u32| if n == 0 { "call-".to_string() } else { format!("call{n}-") })
        .find(|p| tool_names.iter().all(|t| !schema_taken(&format!("{p}{t}"))))
        .unwrap_or_default();
    let tool_root = format!("{prefix}root");
    let base = free_name(&format!("{ANSWER_RULE}-schema-only"), |name| {
        schema_taken(name) || tool_names.iter().any(|t| format!("{prefix}{t}") == name)
    });
    let schema = map_names(schema_gbnf, |name, defining| {
        if defining && name == ANSWER_RULE { base.clone() } else { name.to_string() }
    })?;
    let tools = prefix_rules(tool_gbnf, &prefix)?;
    let mut merged = String::with_capacity(schema.len() + tools.len() + 96);
    for part in [schema.trim_end(), tools.trim_end()] {
        merged.push_str(part);
        merged.push('\n');
    }
    merged.push_str(&format!("{ANSWER_RULE} ::= {base} | {tool_root}\n"));
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Shaped like llama.cpp's strict render with a schema (autoparser: thinking, optional fence)
    const SCHEMA: &str = r#"root ::= ("<think>" until-3 "</think>")? space ("```json" space response-format space "```" | space response-format space)
response-format ::= response-format-schema
response-format-schema ::= "{" space name-kv "}" space
name-kv ::= "\"name\"" space ":" space string
space ::= | " " | "\n"{1,2} [ \t]{0,20}
string ::= "\"" char* "\"" space
char ::= [^"\\\x7F\x00-\x1F] | [\\] (["\\bfnrt] | "u" [0-9a-fA-F]{4})
until-3 ::= ([^<] | "<" [^/])*
"#;

    // Shaped like llama.cpp's lazy render with tools (root = the trigger rules only)
    const TOOLS: &str = r#"root ::= tool-call
tool-call ::= "<tool_call>" space tool-0 space "</tool_call>"
tool-0 ::= "{" space "\"name\"" space ":" space "\"get_weather\"" space "," space "\"arguments\"" space ":" space tool-0-args "}" space
tool-0-args ::= "{" space "\"city\"" space ":" space string "}" space
space ::= | " " | "\n"{1,2} [ \t]{0,20}
string ::= "\"" char* "\"" space
char ::= [^"\\\x7F\x00-\x1F] | [\\] (["\\bfnrt] | "u" [0-9a-fA-F]{4})
until-3 ::= [^<]*
"#;

    #[test]
    fn rule_names_skip_strings_classes_tokens_counts_and_comments() {
        let gbnf = "root ::= \"a b ::= c\" [x-y] <|im_end|> !<[42]> x{1,3} # z ::= w\nx ::= \"\\\"q\"\n";
        assert_eq!(rule_names(gbnf).unwrap(), vec!["root", "x"]);
    }

    #[test]
    fn prefix_renames_definitions_and_references_only() {
        let gbnf = "root ::= a \"a\" [a] a{2}\na ::= \"x\" # a\n";
        assert_eq!(prefix_rules(gbnf, "p-").unwrap(), "p-root ::= p-a \"a\" [a] p-a{2}\np-a ::= \"x\" # a\n");
        assert!(prefix_rules(gbnf, "bad_prefix").is_err());
        assert!(prefix_rules("root ::= \"open", "p-").is_err());
    }

    #[test]
    fn merge_adds_the_call_where_the_answer_starts() {
        let merged = answer_or_tool_call(SCHEMA, TOOLS).unwrap();
        let names = rule_names(&merged).unwrap();
        // One root (the schema's), every name defined once
        assert_eq!(names.iter().filter(|n| *n == "root").count(), 1);
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "a rule is defined twice: {names:?}");
        assert!(merged.contains("response-format-schema-only ::= response-format-schema\n"));
        assert!(merged.ends_with("response-format ::= response-format-schema-only | call-root\n"));
        assert!(merged.contains("call-root ::= call-tool-call\n"));
        assert!(merged.contains(r#"call-tool-0-args ::= "{" call-space "\"city\"" call-space ":" call-space call-string "}" call-space"#));
        // The thinking block and the fence stay as llama.cpp wrote them
        assert!(merged.starts_with("root ::= (\"<think>\" until-3 \"</think>\")? space (\"```json\""));
        // Same-named helper rules stay apart (`until-3` differs in the two grammars)
        assert!(merged.contains("until-3 ::= ([^<] | \"<\" [^/])*"));
        assert!(merged.contains("call-until-3 ::= [^<]*"));
    }

    #[test]
    fn every_reference_has_a_definition() {
        let merged = answer_or_tool_call(SCHEMA, TOOLS).unwrap();
        let defined = rule_names(&merged).unwrap();
        let mut undefined = Vec::new();
        map_names(&merged, |name, defining| {
            if !defining && !defined.iter().any(|d| d == name) {
                undefined.push(name.to_string());
            }
            name.to_string()
        })
        .unwrap();
        assert!(undefined.is_empty(), "undefined rules: {undefined:?}");
    }

    #[test]
    fn taken_names_get_free_ones() {
        let schema = format!("{SCHEMA}call-root ::= \"x\"\nresponse-format-schema-only ::= \"y\"\n");
        let merged = answer_or_tool_call(&schema, TOOLS).unwrap();
        assert!(merged.ends_with("response-format ::= response-format-schema-only1 | call1-root\n"));
        assert!(merged.contains("call-root ::= \"x\""));
    }

    #[test]
    fn merge_needs_the_answer_rule_and_both_roots() {
        assert!(answer_or_tool_call("root ::= object\nobject ::= \"{}\"\n", TOOLS).is_err());
        assert!(answer_or_tool_call(SCHEMA, "tool-call ::= \"<tool_call>\"\n").is_err());
        assert!(answer_or_tool_call("response-format ::= \"{}\"\n", TOOLS).is_err());
    }

    #[test]
    fn only_tag_word_triggers_merge() {
        assert!(triggers_allow_merge(&json!([{"type": "word", "value": "<tool_call>", "token": -1}])));
        assert!(triggers_allow_merge(&json!([{"type": "word", "value": "[TOOL_CALLS]"}])));
        assert!(!triggers_allow_merge(&json!([{"type": "word", "value": "[{\"name\""}])));
        assert!(!triggers_allow_merge(&json!([{"type": "word", "value": "<tool_call>"}, {"type": "word", "value": "{"}])));
        assert!(!triggers_allow_merge(&json!([{"type": "word", "value": "{\"type\": \"function\","}])));
        assert!(!triggers_allow_merge(&json!([{"type": "pattern", "value": "^<\\|channel\\|>commentary"}])));
        assert!(!triggers_allow_merge(&json!([])));
        assert!(!triggers_allow_merge(&json!(null)));
    }
}
