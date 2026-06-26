use super::ast::{CelAst, CelOp, CelPipeline, CelStatement, CelValue, CompareOp, Filter};
use std::collections::HashMap;

/// Maximum nesting depth for if/foreach blocks.
/// Prevents stack overflow via deeply nested CEL input (C-2 security fix).
const MAX_PARSE_DEPTH: usize = 32;

/// Plugin name allowlist: alphanumeric, hyphen, underscore only.
/// Prevents path traversal and injection via `use plugin::` directive (M-3 security fix).
fn validate_plugin_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Plugin name cannot be empty.".to_string());
    }
    if name.len() > 128 {
        return Err(format!(
            "Plugin name '{}' exceeds max length (128 chars). Got {} chars.",
            name, name.len()
        ));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(format!(
            "Plugin name '{}' contains invalid characters. \
             Only alphanumeric, hyphen (-), and underscore (_) are permitted. \
             Path separators and special characters are not allowed.",
            name
        ));
    }
    Ok(())
}

/// Parses a raw CEL string into a full Turing-Complete `CelAst`.
/// Enforces a maximum nesting depth of `MAX_PARSE_DEPTH` to prevent stack overflows.
pub fn parse(input: &str) -> Result<CelAst, String> {
    parse_inner(input.trim(), 0)
}

fn parse_inner(input: &str, depth: usize) -> Result<CelAst, String> {
    if input.is_empty() {
        return Err("Empty CEL expression".to_string());
    }
    if depth > MAX_PARSE_DEPTH {
        return Err(format!(
            "CEL nesting depth exceeded (max {} levels). \
             Deeply nested if/foreach expressions are not permitted.",
            MAX_PARSE_DEPTH
        ));
    }

    let mut ast = CelAst::new();
    
    // Naive statement splitting for MVP (doesn't handle semicolons inside quotes/braces perfectly yet)
    let raw_statements = split_statements(input);

    for stmt_str in raw_statements {
        let stmt_str = stmt_str.trim();
        if stmt_str.is_empty() { continue; }

        if stmt_str.starts_with("let ") {
            let eq_pos = stmt_str.find('=').ok_or("Expected '=' in let assignment")?;
            let var_name = stmt_str[4..eq_pos].trim().to_string();
            let pipeline_str = stmt_str[eq_pos + 1..].trim();
            let pipeline = parse_pipeline(pipeline_str)?;
            ast.statements.push(CelStatement::Assignment { var_name, pipeline });
        } 
        else if stmt_str.starts_with("if ") {
            // MVP if/else parsing
            let start_paren = stmt_str.find('(').ok_or("Expected '(' after if")?;
            let end_paren = stmt_str.find(')').ok_or("Expected ')' after condition")?;
            let condition = stmt_str[start_paren + 1..end_paren].trim().to_string();
            
            let start_brace = stmt_str.find('{').ok_or("Expected '{' for if block")?;
            let end_brace = find_closing_brace(stmt_str, start_brace)?;
            let if_body = &stmt_str[start_brace + 1..end_brace];
            let if_ast = parse_inner(if_body, depth + 1)?;

            let mut else_ast = None;
            let remainder = stmt_str[end_brace + 1..].trim();
            if remainder.starts_with("else") {
                let else_start_brace = remainder.find('{').ok_or("Expected '{' for else block")?;
                let else_end_brace = find_closing_brace(remainder, else_start_brace)?;
                let else_body = &remainder[else_start_brace + 1..else_end_brace];
                else_ast = Some(Box::new(parse_inner(else_body, depth + 1)?));
            }

            ast.statements.push(CelStatement::IfElse { 
                condition, 
                if_block: Box::new(if_ast), 
                else_block: else_ast 
            });
        }
        else if stmt_str.starts_with("foreach ") {
            let start_paren = stmt_str.find('(').ok_or("Expected '(' after foreach")?;
            let end_paren = stmt_str.find(')').ok_or("Expected ')' after foreach")?;
            let in_clause = &stmt_str[start_paren + 1..end_paren];
            let parts: Vec<&str> = in_clause.split(" in ").collect();
            if parts.len() != 2 {
                return Err("foreach must use 'item in list' format".to_string());
            }
            
            let start_brace = stmt_str.find('{').ok_or("Expected '{' for foreach block")?;
            let end_brace = find_closing_brace(stmt_str, start_brace)?;
            let block_body = &stmt_str[start_brace + 1..end_brace];
            let block_ast = parse_inner(block_body, depth + 1)?;

            ast.statements.push(CelStatement::Foreach { 
                item_var: parts[0].trim().to_string(), 
                list_var: parts[1].trim().to_string(), 
                block: Box::new(block_ast) 
            });
        }
        else {
            // Default to linear pipeline Expression
            let pipeline = parse_pipeline(stmt_str)?;
            ast.statements.push(CelStatement::Expression(pipeline));
        }
    }

    Ok(ast)
}

fn split_statements(input: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut brace_depth = 0;

    for c in input.chars() {
        if c == '{' { brace_depth += 1; }
        if c == '}' { brace_depth -= 1; }

        if c == ';' && brace_depth == 0 {
            statements.push(current.clone());
            current.clear();
        } else {
            current.push(c);
        }
    }
    if !current.trim().is_empty() {
        statements.push(current);
    }
    statements
}

fn find_closing_brace(input: &str, start_idx: usize) -> Result<usize, String> {
    let mut depth = 0;
    for (i, c) in input[start_idx..].char_indices() {
        if c == '{' { depth += 1; }
        if c == '}' {
            depth -= 1;
            if depth == 0 {
                return Ok(start_idx + i);
            }
        }
    }
    Err("Missing closing brace '}'".to_string())
}

fn parse_pipeline(input: &str) -> Result<CelPipeline, String> {
    let mut pipeline = CelPipeline::new();
    let segments: Vec<&str> = input.split("->").map(|s| s.trim()).collect();

    // ── Hardcore Engine Directives Interception ──
    if !segments.is_empty() && segments[0] == "engine" {
        if segments.len() >= 3 && segments[1] == "kv_cache" {
            let action_body = segments[2]; // e.g. clear($user_id)
            let action = action_body.split('(').next().unwrap_or("").trim().to_string();
            let start = action_body.find('(').unwrap_or(0);
            let end = action_body.rfind(')').unwrap_or(action_body.len());
            let target = if start < end { action_body[start+1..end].trim().to_string() } else { "".to_string() };
            pipeline.ops.push(CelOp::EngineMemoryControl { action, target });
            return Ok(pipeline);
        } else if segments.len() >= 3 && segments[1] == "mid_layer" {
            let action_body = segments[2]; // e.g. inject($data)
            let start = action_body.find('(').unwrap_or(0);
            let end = action_body.rfind(')').unwrap_or(action_body.len());
            let payload_str = if start < end { action_body[start+1..end].trim() } else { "" };
            let payload = parse_value(payload_str).unwrap_or(CelValue::Null);
            pipeline.ops.push(CelOp::MidLayerInjection { payload });
            return Ok(pipeline);
        } else if segments.len() >= 3 && segments[1] == "inference" {
            let action_body = segments[2]; // e.g. pause()
            let command = action_body.split('(').next().unwrap_or("").trim().to_string();
            pipeline.ops.push(CelOp::InferenceControl { command });
            return Ok(pipeline);
        } else if segments.len() >= 3 && segments[1] == "os" {
            let action_body = segments[2]; // e.g. process("ps")
            let command = action_body.split('(').next().unwrap_or("").trim().to_string();
            let start = action_body.find('(').unwrap_or(0);
            let end = action_body.rfind(')').unwrap_or(action_body.len());
            let args_str = if start < end { action_body[start+1..end].trim() } else { "" };
            let mut args = Vec::new();
            if !args_str.is_empty() {
                args.push(parse_value(args_str).unwrap_or(CelValue::Null));
            }
            // M-1: SystemCall is emitted here. The planner/executor MUST verify
            // that EngineRules.allow_subprocess == Some(true) before executing this op.
            // The parser does not enforce permissions — that is the executor's responsibility.
            pipeline.ops.push(CelOp::SystemCall { command, args });
            return Ok(pipeline);
        }
    }

    for segment in segments {
        if segment.is_empty() { continue; }
        if segment.starts_with("use plugin::") {
            let name = segment["use plugin::".len()..].trim().to_string();
            // M-3: Validate plugin name against allowlist — prevents path traversal and injection
            validate_plugin_name(&name)?;
            pipeline.ops.push(CelOp::ImportPlugin { name });
        } else if segment.starts_with("process(") {
            let body = segment.strip_prefix("process(").and_then(|s| s.strip_suffix(')')).unwrap_or("");
            let text = body.trim_matches('\'').trim_matches('"').to_string();
            pipeline.ops.push(CelOp::FastProcess {
                method: "process".to_string(),
                payload_text: text,
            });
        } else if segment.starts_with("invoke(") {
            let body = segment.strip_prefix("invoke(").and_then(|s| s.strip_suffix(')')).unwrap_or("");
            let method = extract_named_string(body, "method").unwrap_or_else(|| "default".to_string());
            let mut args = HashMap::new();
            if let Some(val) = extract_named_string(body, "payload") {
                args.insert("payload".to_string(), parse_value(&val).unwrap_or(CelValue::Text(val)));
            }
            pipeline.ops.push(CelOp::InvokeAction { method, args });
        } else if segment.starts_with("filter ") {
            let filter_body = &segment["filter ".len()..];
            let filters = parse_filters(filter_body)?;
            if let Some(f) = filters.into_iter().next() {
                pipeline.ops.push(CelOp::Filter { field: f.field, op: f.op, value: f.value });
            }
        } else if segment.starts_with("select(") {
            let body = segment.strip_prefix("select(").and_then(|s| s.strip_suffix(')')).unwrap_or("");
            let fields: Vec<String> = body.split(',').map(|s| s.trim().to_string()).collect();
            pipeline.ops.push(CelOp::Select { fields });
        } else if segment.starts_with("time_window(") {
            let body = segment.strip_prefix("time_window(").and_then(|s| s.strip_suffix(')')).unwrap_or("");
            let size = extract_named_string(body, "size").unwrap_or_else(|| "1h".to_string());
            pipeline.ops.push(CelOp::TimeWindow { size });
        } else if segment.starts_with("similar_to(") {
            let body = segment.strip_prefix("similar_to(").and_then(|s| s.strip_suffix(')')).unwrap_or("");
            let metric = extract_named_string(body, "metric").unwrap_or_else(|| "cosine".to_string());
            let vector = extract_float_array(body, "vector").unwrap_or_else(|| vec![]);
            pipeline.ops.push(CelOp::SimilarTo { vector, metric }); 
        } else {
            if let Some(command) = parse_generic_command(segment) {
                pipeline.ops.push(command);
            } else {
                pipeline.ops.push(CelOp::Pipe { next_plugin: segment.to_string() });
            }
        }
    }

    Ok(pipeline)
}

fn parse_value(input: &str) -> Result<CelValue, String> {
    let input = input.trim();
    if input == "?" { return Ok(CelValue::Parameter); }
    if input.starts_with('$') { return Ok(CelValue::Variable(input.to_string())); }
    if input.starts_with('"') && input.ends_with('"') {
        return Ok(CelValue::Text(input[1..input.len() - 1].to_string()));
    }
    if input == "true" { return Ok(CelValue::Bool(true)); }
    if input == "false" { return Ok(CelValue::Bool(false)); }
    if input == "null" { return Ok(CelValue::Null); }
    if let Ok(n) = input.parse::<f64>() { return Ok(CelValue::Number(n)); }
    Ok(CelValue::Text(input.to_string()))
}

fn parse_filters(input: &str) -> Result<Vec<Filter>, String> {
    let input = input.trim();
    if input.is_empty() { return Ok(vec![]); }

    let mut filters = Vec::new();
    let parts: Vec<&str> = input.split(',').collect(); 

    for part in parts {
        if part.trim().is_empty() { continue; }
        filters.push(parse_single_filter(part.trim())?);
    }

    Ok(filters)
}

fn parse_single_filter(input: &str) -> Result<Filter, String> {
    let ops = [
        (">=", CompareOp::Gte),
        ("<=", CompareOp::Lte),
        ("!=", CompareOp::NotEq),
        (">", CompareOp::Gt),
        ("<", CompareOp::Lt),
        ("contains", CompareOp::Contains),
        (":", CompareOp::Eq),
        ("=", CompareOp::Eq),
    ];

    for (op_str, op) in &ops {
        if let Some(pos) = input.find(op_str) {
            let field = input[..pos].trim().to_string();
            let value_str = input[pos + op_str.len()..].trim();
            let value = parse_value(value_str)?;
            return Ok(Filter { field, op: op.clone(), value });
        }
    }
    Err(format!("Cannot parse filter: '{}'", input))
}

fn extract_named_string(body: &str, key: &str) -> Option<String> {
    let search = format!("{}:", key);
    let pos = body.find(search.as_str())?;
    let after = body[pos + search.len()..].trim();
    if after.starts_with('"') {
        let inner = &after[1..];
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        let raw = after.split(',').next().unwrap_or("").trim();
        if raw.is_empty() { None } else { Some(raw.to_string()) }
    }
}

fn extract_float_array(body: &str, key: &str) -> Option<Vec<f32>> {
    let search = format!("{}:", key);
    let pos = body.find(search.as_str())?;
    let after = body[pos + search.len()..].trim();
    if !after.starts_with('[') { return None; }
    let inner = &after[1..];
    let end = inner.find(']')?;
    let array_str = &inner[..end];
    
    let floats: Vec<f32> = array_str.split(',').filter_map(|s| s.trim().parse::<f32>().ok()).collect();
    Some(floats)
}

fn parse_generic_command(segment: &str) -> Option<CelOp> {
    let mut target = None;
    let mut args = HashMap::new();

    let space_idx = segment.find(' ')?;
    let action = segment[..space_idx].trim().to_string();

    
    let remainder = segment[space_idx..].trim();
    
    if let Some(paren_start) = remainder.find('(') {
        let potential_target = remainder[..paren_start].trim();
        if !potential_target.is_empty() {
            target = Some(potential_target.to_string());
        }
        
        if let Some(paren_end) = remainder.rfind(')') {
            let args_str = &remainder[paren_start + 1..paren_end];
            for part in args_str.split(',') {
                let part = part.trim();
                if part.is_empty() { continue; }
                if let Some(colon_idx) = part.find(':') {
                    let key = part[..colon_idx].trim().to_string();
                    let val_str = part[colon_idx + 1..].trim();
                    args.insert(key, parse_value(val_str).unwrap_or(CelValue::Text(val_str.to_string())));
                }
            }
        }
    } else {
        if !remainder.is_empty() {
            target = Some(remainder.to_string());
        }
    }

    if action.is_empty() { return None; }

    Some(CelOp::Command { action, target, args })
}
