//! `cluaiz flow`: Automation from the terminal, through the running server's `flows.*` methods
//! (docs/automation/guide.md#ways-to-build-one).

use crate::cli::server::{cep, cep_reply};
use clap::Subcommand;
use color_eyre::eyre::{eyre, Result};
use colored::Colorize;
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::Path;

/// How long one `tasks.result` call waits (the request itself times out after 10 s)
const RESULT_WAIT_MS: u64 = 5_000;

#[derive(Subcommand)]
pub enum FlowCommand {
    /// The flows of the project and your own, with their last run
    #[command(alias = "ls")]
    List,
    /// Check a flow file (.flow.json, .flow.yaml) without saving it
    Validate { file: String },
    /// Save a flow file; you confirm the permissions it needs
    Save {
        file: String,
        /// Where to save it: user (default) or project
        #[arg(long)]
        scope: Option<String>,
        /// Replace a flow of the same name
        #[arg(long)]
        overwrite: bool,
    },
    /// Run a saved flow and wait for it
    Run {
        name: String,
        /// An input: name=value (a value that is JSON is read as JSON)
        #[arg(long = "input", short = 'i', value_name = "NAME=VALUE")]
        inputs: Vec<String>,
        /// Run without side effects: steps that write, send or run programs are not called
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        scope: Option<String>,
    },
    /// Run a failed run again from one step; the steps before it are not run again
    Rerun {
        run_id: String,
        /// The step id to start from
        #[arg(long)]
        from: String,
        /// Replace an input: name=value
        #[arg(long = "input", short = 'i', value_name = "NAME=VALUE")]
        inputs: Vec<String>,
    },
    /// The last runs of a flow, or one run's steps with --run
    Logs {
        name: String,
        #[arg(long)]
        run: Option<String>,
    },
    /// Turn a flow's triggers on (schedules, folders, webhooks…)
    Enable {
        name: String,
        #[arg(long)]
        scope: Option<String>,
    },
    /// Turn a flow's triggers off; it can still be run by hand
    Disable {
        name: String,
        #[arg(long)]
        scope: Option<String>,
    },
    /// Remove a flow and its runs
    #[command(alias = "rm")]
    Remove {
        name: String,
        #[arg(long)]
        scope: Option<String>,
    },
    /// Write a flow from a description with a model; it is checked and tested, not saved
    Draft {
        /// What the flow should do, in plain words, in any language
        request: String,
        /// Write the flow to this file (then `cluaiz flow save <file>`)
        #[arg(long, short = 'o')]
        out: Option<String>,
        /// The model that writes it (default the default chat model)
        #[arg(long)]
        model: Option<String>,
        /// Do not test it with a dry run
        #[arg(long)]
        no_test: bool,
    },
    /// A flow in plain words: a saved flow's name, or a flow file
    Explain {
        flow: String,
        /// Let a model write it more naturally
        #[arg(long)]
        model: bool,
        /// With --model: the language to write in
        #[arg(long)]
        language: Option<String>,
        #[arg(long)]
        scope: Option<String>,
    },
    /// A model reads a failed run and proposes a corrected flow; nothing is saved
    Fix {
        run_id: String,
        /// What you know about the failure
        #[arg(long)]
        note: Option<String>,
        /// Write the corrected flow to this file
        #[arg(long, short = 'o')]
        out: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
    /// What steps can use here: installed tools and methods
    Nodes {
        /// Words to look for in names and descriptions
        query: Option<String>,
    },
}

fn with_scope(mut params: Value, scope: Option<String>) -> Value {
    if let Some(scope) = scope {
        params["scope"] = json!(scope);
    }
    params
}

fn read_file(file: &str) -> Result<(String, &'static str)> {
    let text = std::fs::read_to_string(file).map_err(|e| eyre!("cannot read {file}: {e}"))?;
    let format = match Path::new(file).extension().and_then(|e| e.to_str()) {
        Some("json") => "json",
        Some("yaml") | Some("yml") => "yaml",
        _ => return Err(eyre!("{file}: a flow file ends in .flow.json, .flow.yaml or .flow.yml")),
    };
    Ok((text, format))
}

fn print_problems(problems: &Value) {
    for problem in problems.as_array().into_iter().flatten() {
        let severity = problem["severity"].as_str().unwrap_or("error");
        let mark = if severity == "error" { "error".red().bold() } else { "warning".yellow().bold() };
        let at = match problem["line"].as_u64() {
            Some(line) => format!("line {line}"),
            None => problem["path"].as_str().unwrap_or_default().to_string(),
        };
        println!("  {mark} {} {at}: {}", problem["code"].as_str().unwrap_or_default().dimmed(), problem["message"].as_str().unwrap_or_default());
    }
}

fn print_triggers(triggers: &Value) {
    for trigger in triggers.as_array().into_iter().flatten() {
        let status = trigger["status"].as_str().unwrap_or_default();
        let shown = match status {
            "active" => status.green().to_string(),
            "error" | "waiting_for_permission" => status.red().to_string(),
            _ => status.dimmed().to_string(),
        };
        let next = trigger["next_at"].as_str().map(|t| format!("  next {t}")).unwrap_or_default();
        let path = trigger["path"].as_str().map(|p| format!("  {p}")).unwrap_or_default();
        let problem = trigger["message"].as_str().or(trigger["last_error"]["message"].as_str()).map(|m| format!("  {}", m.red())).unwrap_or_default();
        println!("  {:<16} {:<9} {shown}{next}{path}{problem}", trigger["id"].as_str().unwrap_or_default(), trigger["kind"].as_str().unwrap_or_default());
    }
}

fn input_value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

fn input_map(inputs: &[String]) -> Result<Map<String, Value>> {
    let mut map = Map::new();
    for pair in inputs {
        let (key, value) = pair.split_once('=').ok_or_else(|| eyre!("--input {pair}: write name=value"))?;
        map.insert(key.to_string(), input_value(value));
    }
    Ok(map)
}

/// Waits for a run that `flows.run` or `flows.rerun_from` started, and prints how it ended
async fn wait_for_run(started: Value) -> Result<()> {
    let task_id = started["task"]["task_id"].as_str().ok_or_else(|| eyre!("flows.run did not return a task"))?.to_string();
    println!("run {} started", started["run_id"].as_str().unwrap_or_default().bold());
    let run = loop {
        match cep_reply("tasks.result", json!({ "task_id": task_id, "wait_ms": RESULT_WAIT_MS })).await? {
            Ok(run) => break run,
            Err(error) if error["data"]["type"] == "timeout" => continue,
            Err(error) => return Err(eyre!("tasks.result: {}", error["message"].as_str().unwrap_or("failed"))),
        }
    };
    let status = run["status"].as_str().unwrap_or_default();
    let shown = if status == "succeeded" { status.green().bold() } else { status.red().bold() };
    println!("{shown} in {} ms, {} steps", run["running_ms"], run["steps_done"]);
    if !run["outputs"].is_null() {
        println!("{}", serde_json::to_string_pretty(&run["outputs"]).unwrap_or_default());
    }
    if let Some(error) = run["error"].as_object() {
        println!("{} {} at {}: {}", "error".red(), error["kind"].as_str().unwrap_or_default(), error["step"].as_str().unwrap_or("-"), error["message"].as_str().unwrap_or_default());
    }
    Ok(())
}

/// Waits for a task of `flows.draft` or `flows.fix`; `Err` holds the method's error object
async fn task_outcome(started: &Value, method: &str) -> Result<std::result::Result<Value, Value>> {
    let task_id = started["task"]["task_id"].as_str().ok_or_else(|| eyre!("{method} did not return a task"))?.to_string();
    loop {
        match cep_reply("tasks.result", json!({ "task_id": task_id, "wait_ms": RESULT_WAIT_MS })).await? {
            Err(error) if error["data"]["type"] == "timeout" => continue,
            outcome => return Ok(outcome),
        }
    }
}

fn write_out(out: &str, text: &str) -> Result<()> {
    std::fs::write(out, text).map_err(|e| eyre!("cannot write {out}: {e}"))?;
    println!("written to {}; save it with `cluaiz flow save {out}`", out.bold());
    Ok(())
}

fn print_explanation(explanation: &Value) {
    println!("{}", explanation["summary"].as_str().unwrap_or_default().bold());
    for trigger in explanation["triggers"].as_array().into_iter().flatten() {
        println!("  {} {}", "on".dimmed(), trigger.as_str().unwrap_or_default());
    }
    for step in explanation["steps"].as_array().into_iter().flatten() {
        let depth = step["depth"].as_u64().unwrap_or(0) as usize;
        println!("  {}{} {}", "  ".repeat(depth), step["id"].as_str().unwrap_or_default().dimmed(), step["text"].as_str().unwrap_or_default());
    }
    for touch in explanation["touches"].as_array().into_iter().flatten() {
        println!("  {} {}", "uses".dimmed(), touch.as_str().unwrap_or_default());
    }
    for warning in explanation["warnings"].as_array().into_iter().flatten() {
        println!("  {} {}", "warning".yellow(), warning.as_str().unwrap_or_default());
    }
}

pub async fn execute(command: FlowCommand) -> Result<()> {
    match command {
        FlowCommand::List => {
            let listed = cep("flows.list", json!({})).await?;
            let flows = listed["flows"].as_array().cloned().unwrap_or_default();
            if flows.is_empty() {
                println!("No flows yet. Save one with `cluaiz flow save <file>`.");
            }
            for flow in flows {
                let last = match flow["last_run"]["status"].as_str() {
                    Some(status) => format!("last run {status}"),
                    None => "never run".into(),
                };
                let broken = if flow.get("problems").is_some() { " (does not parse)".red().to_string() } else { String::new() };
                let state = if flow["enabled"] == json!(true) { "on".green().to_string() } else { "off".dimmed().to_string() };
                println!("{:<28} {:<8} {state:<4} {}{broken}  {}", flow["name"].as_str().unwrap_or_default().bold(), flow["scope"].as_str().unwrap_or_default(), last.dimmed(), flow["description"].as_str().unwrap_or_default());
                print_triggers(&flow["triggers"]);
            }
        }
        FlowCommand::Validate { file } => {
            let (text, format) = read_file(&file)?;
            let report = cep("flows.validate", json!({ "text": text, "format": format })).await?;
            if report["valid"] == json!(true) {
                println!("{} {file}", "valid".green().bold());
            } else {
                println!("{} {file}", "not valid".red().bold());
            }
            print_problems(&report["problems"]);
            if let Some(needed) = report["permissions_needed"].as_object().filter(|n| !n.is_empty()) {
                println!("  permissions it needs: {}", Value::Object(needed.clone()));
            }
        }
        FlowCommand::Save { file, scope, overwrite } => {
            let (text, format) = read_file(&file)?;
            let mut params = with_scope(json!({ "text": text, "format": format, "overwrite": overwrite }), scope);
            let saved = match cep_reply("flows.save", params.clone()).await? {
                Ok(saved) => saved,
                Err(error) if error["data"]["type"] == "permissions_unconfirmed" => {
                    println!("This flow needs these permissions:");
                    println!("  {}", serde_json::to_string_pretty(&error["data"]["permissions_needed"]).unwrap_or_default());
                    print!("Allow them? [y/N] ");
                    std::io::stdout().flush()?;
                    let mut answer = String::new();
                    std::io::stdin().read_line(&mut answer)?;
                    if !matches!(answer.trim(), "y" | "Y" | "yes") {
                        return Err(eyre!("not saved: the permissions were not allowed"));
                    }
                    params["permissions_confirmed"] = json!(true);
                    cep("flows.save", params).await?
                }
                Err(error) => {
                    print_problems(&error["data"]["problems"]);
                    return Err(eyre!("flows.save: {}", error["message"].as_str().unwrap_or("failed")));
                }
            };
            println!("{} {} ({}) at {}", "saved".green().bold(), saved["flow"]["name"].as_str().unwrap_or_default(), saved["flow"]["scope"].as_str().unwrap_or_default(), saved["flow"]["path"].as_str().unwrap_or_default());
            print_problems(&saved["problems"]);
        }
        FlowCommand::Run { name, inputs, dry_run, scope } => {
            let started = cep("flows.run", with_scope(json!({ "name": name, "inputs": input_map(&inputs)?, "dry_run": dry_run }), scope)).await?;
            wait_for_run(started).await?;
        }
        FlowCommand::Rerun { run_id, from, inputs } => {
            let mut params = json!({ "run_id": run_id, "from": from });
            if !inputs.is_empty() {
                params["inputs"] = json!(input_map(&inputs)?);
            }
            let started = cep("flows.rerun_from", params).await?;
            wait_for_run(started).await?;
        }
        FlowCommand::Logs { name, run } => match run {
            Some(run_id) => {
                let got = cep("flows.run_get", json!({ "run_id": run_id })).await?;
                println!("{} {}", got["run"]["run_id"].as_str().unwrap_or_default().bold(), got["run"]["status"].as_str().unwrap_or_default());
                for step in got["steps"].as_array().into_iter().flatten() {
                    let ms: u64 = step["attempts"].as_array().into_iter().flatten().filter_map(|a| a["ms"].as_u64()).sum();
                    let error = step["error"]["message"].as_str().map(|m| format!("  {}", m.red())).unwrap_or_default();
                    println!("  {:<10} {:<24} {:<6} {ms} ms{error}", step["status"].as_str().unwrap_or_default(), step["id"].as_str().unwrap_or_default(), step["kind"].as_str().unwrap_or_default());
                }
            }
            None => {
                let runs = cep("flows.runs", json!({ "name": name, "limit": 10 })).await?;
                for run in runs["runs"].as_array().into_iter().flatten() {
                    println!("{}  {:<10} {}  {} ms", run["run_id"].as_str().unwrap_or_default(), run["status"].as_str().unwrap_or_default(), run["created_at"].as_str().unwrap_or_default(), run["running_ms"]);
                }
            }
        },
        FlowCommand::Enable { name, scope } => {
            let params = with_scope(json!({ "name": name }), scope);
            let enabled = match cep_reply("flows.enable", params.clone()).await? {
                Ok(enabled) => enabled,
                Err(error) if error["data"]["type"] == "permissions_unconfirmed" => {
                    println!("The file asks for permissions you have not allowed:");
                    println!("  {}", serde_json::to_string_pretty(&error["data"]["permissions_needed"]).unwrap_or_default());
                    print!("Allow them? [y/N] ");
                    std::io::stdout().flush()?;
                    let mut answer = String::new();
                    std::io::stdin().read_line(&mut answer)?;
                    if !matches!(answer.trim(), "y" | "Y" | "yes") {
                        return Err(eyre!("not enabled: the permissions were not allowed"));
                    }
                    let mut params = params;
                    params["permissions_confirmed"] = json!(true);
                    cep("flows.enable", params).await?
                }
                Err(error) => return Err(eyre!("flows.enable: {}", error["message"].as_str().unwrap_or("failed"))),
            };
            println!("{} {}", "enabled".green().bold(), name);
            print_triggers(&enabled["triggers"]);
        }
        FlowCommand::Disable { name, scope } => {
            let disabled = cep("flows.disable", with_scope(json!({ "name": name }), scope)).await?;
            println!("{} {} (it can still be run by hand)", "disabled".yellow().bold(), name);
            print_triggers(&disabled["flow"]["triggers"]);
        }
        FlowCommand::Remove { name, scope } => {
            let removed = cep("flows.remove", with_scope(json!({ "name": name }), scope)).await?;
            println!("removed {} and {} runs", removed["removed"].as_str().unwrap_or_default(), removed["runs_removed"]);
        }
        FlowCommand::Draft { request, out, model, no_test } => {
            let mut params = json!({ "request": request, "test": !no_test });
            if let Some(model) = model {
                params["model"] = json!(model);
            }
            let started = cep("flows.draft", params).await?;
            println!("{}", "writing the flow…".dimmed());
            let drafted = match task_outcome(&started, "flows.draft").await? {
                Ok(drafted) => drafted,
                Err(error) => {
                    print_problems(&error["data"]["problems"]);
                    if let Some(text) = error["data"]["text"].as_str().filter(|t| !t.is_empty()) {
                        println!("{}\n{text}", "the model's last answer:".dimmed());
                    }
                    return Err(eyre!("flows.draft: {}", error["message"].as_str().unwrap_or("failed")));
                }
            };
            print_explanation(&drafted["explanation"]);
            print_problems(&drafted["problems"]);
            for missing in drafted["missing"].as_array().into_iter().flatten() {
                println!("  {} {} is not installed", "missing".red(), missing["need"].as_str().unwrap_or_default());
            }
            match drafted["test"]["status"].as_str() {
                Some("succeeded") => println!("  {} the dry run succeeded", "test".green()),
                Some(status) => println!("  {} the dry run ended {status}: {}", "test".red(), drafted["test"]["error"]["message"].as_str().or(drafted["test"]["error"].as_str()).unwrap_or_default()),
                None => {}
            }
            let text = drafted["text"].as_str().unwrap_or_default();
            match out {
                Some(out) => write_out(&out, text)?,
                None => println!("\n{text}"),
            }
        }
        FlowCommand::Explain { flow, model, language, scope } => {
            let mut params = match read_file(&flow) {
                Ok((text, format)) if Path::new(&flow).is_file() => json!({ "text": text, "format": format }),
                _ => with_scope(json!({ "name": flow }), scope),
            };
            if model {
                params["mode"] = json!("model");
            }
            if let Some(language) = language {
                params["language"] = json!(language);
            }
            print_explanation(&cep("flows.explain", params).await?);
        }
        FlowCommand::Fix { run_id, note, out, model } => {
            let mut params = json!({ "run_id": run_id });
            if let Some(note) = note {
                params["note"] = json!(note);
            }
            if let Some(model) = model {
                params["model"] = json!(model);
            }
            let started = cep("flows.fix", params).await?;
            println!("{}", "reading the failed run…".dimmed());
            let fixed = task_outcome(&started, "flows.fix").await?.map_err(|error| eyre!("flows.fix: {}", error["message"].as_str().unwrap_or("failed")))?;
            println!("{}", fixed["diagnosis"].as_str().unwrap_or_default().bold());
            if fixed["cause_outside_flow"] == json!(true) {
                println!("  nothing in the flow can fix this; the diagnosis says what to do");
                return Ok(());
            }
            for change in fixed["changes"].as_array().into_iter().flatten() {
                println!("  {} {}  {} → {}", "change".cyan(), change["path"].as_str().unwrap_or_default(), change["before"], change["after"]);
                if let Some(why) = change["why"].as_str() {
                    println!("      {}", why.dimmed());
                }
            }
            print_problems(&fixed["problems"]);
            if let Some(out) = out {
                let text = serde_json::to_string_pretty(&fixed["flow"]).unwrap_or_default() + "\n";
                write_out(&out, &text)?;
            }
        }
        FlowCommand::Nodes { query } => {
            let mut params = json!({ "schemas": false, "catalog": false });
            if let Some(query) = query {
                params["query"] = json!(query);
            }
            let listed = cep("flows.nodes", params).await?;
            for node in listed["nodes"].as_array().into_iter().flatten() {
                let description: String = node["description"].as_str().unwrap_or_default().chars().take(90).collect();
                println!("{:<5} {:<40} {}", node["step"].as_str().unwrap_or_default().dimmed(), node["ref"].as_str().unwrap_or_default(), description);
            }
        }
    }
    Ok(())
}
