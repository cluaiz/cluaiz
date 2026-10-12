//! Interactive chat on plain stdin / stdout (the terminal's own line editing, no screen takeover),
//! with ollama-style `/` commands.

use std::io::{BufRead, Write};

use cluaiz_core::{FeatureState, ThinkingBudget};
use cluaiz_models::DownloadEvent;
use color_eyre::{eyre::eyre, Result};
use colored::Colorize;
use tokio::sync::mpsc;

use super::{print, Session};
use crate::cli::keys;
use crate::core::state::AppState;

const HELP: &str = "\
Commands:
  /set parameter <name> <value>  temp, top_k, top_p, min_p, repeat_penalty, presence_penalty,
                                 frequency_penalty, seed, n_predict (value `default` = unset)
  /set system <text>             System prompt
  /set think <on|off|auto>       Thinking of reasoning models
  /set effort <level|N>          Thinking budget: auto, low, medium, high, max or tokens
  /set budget-message <text>     Text written when the thinking budget runs out
  /set verbose | quiet           Show or hide speed after each answer
  /set showthinking | hidethinking
  /show info | plan | parameters | system
  /load <model>                  Switch to another installed model (keeps the conversation)
  /models                        Browse, download or delete models
  /save <file>                   Save the conversation as JSON
  /clear                         Forget the conversation
  /bye                           Exit (also Ctrl+D)
  /?                             This help

Use \"\"\" to begin and end a multi-line message. Ctrl+C stops an answer.";

/// One line from stdin; None at end of input (Ctrl+D, Ctrl+Z on Windows).
fn read_line(prompt: &str) -> Option<String> {
    tokio::task::block_in_place(|| {
        print!("{}", prompt);
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        }
    })
}

/// A message: one line, or several between `"""` marks.
fn read_message() -> Option<String> {
    let first = read_line(&format!("{} ", ">>>".bold()))?;
    let Some(rest) = first.trim_start().strip_prefix("\"\"\"") else {
        return Some(first);
    };
    if let Some(done) = rest.strip_suffix("\"\"\"") {
        return Some(done.to_string());
    }
    let mut lines = vec![rest.to_string()];
    loop {
        let line = read_line("... ")?;
        if let Some(last) = line.strip_suffix("\"\"\"") {
            lines.push(last.to_string());
            return Some(lines.join("\n").trim_matches('\n').to_string());
        }
        lines.push(line);
    }
}

pub async fn run(
    session: &mut Session,
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<DownloadEvent>,
    rx: &mut mpsc::UnboundedReceiver<DownloadEvent>,
    first_message: Option<String>,
) -> Result<()> {
    eprintln!("{}", "Type a message, /? for help, /bye to exit.".dimmed());
    let mut pending = first_message;
    loop {
        let input = match pending.take() {
            Some(m) => {
                println!("{} {}", ">>>".bold(), m);
                m
            }
            None => match read_message() {
                Some(m) => m,
                None => {
                    println!();
                    return Ok(());
                }
            },
        };
        let text = input.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(slash) = text.strip_prefix('/') {
            match command_word(slash) {
                ("bye", _) | ("exit", _) => return Ok(()),
                ("models", _) => {
                    tokio::task::block_in_place(|| crate::ui::apps::registry::RegistryApp::show(state, tx))?;
                    state.handle_events(rx);
                }
                (word, args) => {
                    if let Err(e) = command(session, word, args).await {
                        eprintln!("{} {}", "error:".red(), e);
                    }
                }
            }
            continue;
        }
        let mut out = print::StreamPrinter::new(session.options.show_thinking);
        match session.send(text, &mut out).await {
            Ok(report) => {
                if session.options.verbose || report.cancelled {
                    print::stats(&report);
                }
            }
            Err(e) => eprintln!("{} {}", "error:".red(), e),
        }
        println!();
    }
}

fn command_word(command: &str) -> (&str, &str) {
    match command.split_once(char::is_whitespace) {
        Some((w, rest)) => (w, rest.trim()),
        None => (command, ""),
    }
}

async fn command(session: &mut Session, word: &str, args: &str) -> Result<()> {
    match word {
        "?" | "help" => println!("{}", HELP),
        "clear" => {
            session.history.clear();
            println!("Conversation cleared.");
        }
        "set" => set(session, args)?,
        "show" => show(session, args).await?,
        "load" => {
            if args.is_empty() {
                return Err(eyre!("usage: /load <model>"));
            }
            let (path, id) = crate::cli::models::resolve_installed(args)?;
            session.switch(path, id).await?;
            print::plan(session).await;
        }
        "save" => {
            if args.is_empty() {
                return Err(eyre!("usage: /save <file>"));
            }
            let json = serde_json::to_string_pretty(&serde_json::json!({
                "model": session.model_id,
                "system": session.options.system,
                "messages": session.history,
            }))?;
            std::fs::write(args, json)?;
            println!("Saved {} messages to {}", session.history.len(), args);
        }
        other => return Err(eyre!("unknown command /{} (see /?)", other)),
    }
    Ok(())
}

fn set(session: &mut Session, args: &str) -> Result<()> {
    let (what, value) = command_word(args);
    let o = &mut session.options;
    match what {
        "parameter" => {
            let (name, raw) = command_word(value);
            if raw.is_empty() {
                return Err(eyre!("usage: /set parameter <name> <value>"));
            }
            let unset = matches!(raw, "default" | "auto" | "null");
            if name == "n_predict" {
                o.max_tokens = if unset { None } else { Some(raw.parse().map_err(|_| eyre!("n_predict: a number of tokens"))?) };
            } else {
                let mut samplers = serde_json::to_value(&o.samplers)?;
                let v = if unset { serde_json::Value::Null } else { keys::parse_value(raw) };
                keys::set(&mut samplers, name, v).map_err(|_| eyre!("unknown parameter `{}` (see /?)", name))?;
                o.samplers = serde_json::from_value(samplers).map_err(|e| eyre!("{}: {}", name, e))?;
            }
            println!("Set parameter '{}' to '{}'", name, raw);
        }
        "system" => {
            o.system = Some(value.to_string()).filter(|s| !s.is_empty());
            println!("Set system message.");
        }
        "think" => {
            o.think = Some(match value {
                "on" => FeatureState::On,
                "off" => FeatureState::Off,
                "auto" => FeatureState::Auto,
                _ => return Err(eyre!("usage: /set think <on|off|auto>")),
            });
            println!("Set thinking to {}.", value);
        }
        "effort" => {
            o.effort = Some(ThinkingBudget::parse(value).map_err(|e| eyre!(e))?);
            println!("Set thinking budget to {}.", value);
        }
        "budget-message" => {
            o.budget_message = Some(value.to_string());
            println!("Set thinking budget message.");
        }
        "verbose" => o.verbose = true,
        "quiet" => o.verbose = false,
        "showthinking" => o.show_thinking = true,
        "hidethinking" => o.show_thinking = false,
        _ => return Err(eyre!("unknown setting '{}' (see /?)", what)),
    }
    Ok(())
}

async fn show(session: &Session, what: &str) -> Result<()> {
    match what {
        "info" => {
            let desc = session.host.model_description().await;
            let dna = session.host.dna.lock().await;
            let row = |k: &str, v: String| println!("  {:<16}{}", k, v);
            row("model", session.model_id.clone());
            row("file", session.model_path.display().to_string());
            if let Some(d) = desc {
                row("llama.cpp", d.desc);
                row("size", format!("{:.2} GiB", d.size_bytes as f64 / cluaiz_core::GIB_F64));
                row("parameters", format!("{:.2} B", d.n_params as f64 / 1e9));
            }
            row("context", format!("{} tokens", session.plan.context_window));
            let thinking = if dna.think_tag_schema.is_empty() { "no".to_string() } else { format!("{} ... {}", dna.think_tag_schema, dna.think_end_schema) };
            row("thinking", thinking);
        }
        "plan" => match session.host.plan_report.lock().await.as_ref() {
            Some(banner) => println!("{}", banner),
            None => println!("{}", session.plan.summary()),
        },
        "parameters" => {
            let o = &session.options;
            let mut rows = Vec::new();
            keys::flatten(&serde_json::to_value(&o.samplers)?, "", &mut rows);
            for (k, v) in rows.iter().filter(|(_, v)| !v.is_null()) {
                println!("  {:<18}{}", k, keys::display(v));
            }
            if let Some(n) = o.max_tokens {
                println!("  {:<18}{}", "n_predict", n);
            }
            let settings = cluaiz_core::Settings::current().optimization;
            println!("  {:<18}{}", "think", o.think.unwrap_or(settings.think_mode).name());
            let effort = serde_json::to_value(o.effort.unwrap_or(settings.reasoning_effort))?;
            println!("  {:<18}{}", "effort", keys::display(&effort));
            println!("{}", "  (unlisted samplers use settings.json, then the model's defaults)".dimmed());
        }
        "system" => println!("{}", session.options.system.as_deref().unwrap_or("(no system message)")),
        _ => return Err(eyre!("usage: /show info | plan | parameters | system")),
    }
    Ok(())
}
