//! `cluaiz`, `cluaiz -h` and `cluaiz help`: the mascot banner, then the commands in groups (the
//! descriptions are clap's own, so they never drift from `cluaiz <command> --help`).
//! `cluaiz help all` (or `cluaiz help <command> all`): every command and subcommand, to the full depth.

use clap::CommandFactory;
use colored::Colorize;

/// Groups in the order a new user needs them; a command missing here goes under "More"
const GROUPS: &[(&str, &[&str])] = &[
    ("Chat and agent", &["agent", "run"]),
    ("Models", &["pull", "list", "show", "rm", "model", "provider", "ps"]),
    ("Server", &["serve", "mcp-server", "token", "permission"]),
    ("This machine", &["status", "calibrate", "bench"]),
    ("Tools", &["skill", "plugin", "mcp"]),
    ("Automation", &["flow"]),
    ("Documents and storage", &["ingest", "storage"]),
    ("Settings", &["config"]),
    ("Update and remove", &["self"]),
];

/// `cluaiz help all` / `cluaiz help <command…> all`: every command under `path` to the full depth,
/// each with its arguments and what it does. `cluaiz <command> -h` still lists a command's options.
pub fn print_all<C: CommandFactory>(path: &[String]) -> Result<(), String> {
    let root = C::command();
    let mut command = &root;
    let mut shown_path = String::from("cluaiz");
    for name in path {
        command = shown(command)
            .find(|c| c.get_name() == name || c.get_all_aliases().any(|a| a == name))
            .ok_or_else(|| format!("no command '{name}' in `{shown_path}`; `cluaiz help all` lists every command"))?;
        shown_path = format!("{shown_path} {}", command.get_name());
    }
    if path.is_empty() {
        println!("{} {}", "Every command".bold(), "(`cluaiz <command> -h` for a command's options)".dimmed());
        let mut done = Vec::new();
        for (title, names) in GROUPS {
            let commands: Vec<_> = names.iter().filter_map(|n| shown(&root).find(|c| c.get_name() == *n)).collect();
            if commands.is_empty() {
                continue;
            }
            println!("\n{}", title.bold());
            for c in commands {
                tree(c, "cluaiz", 1);
                done.push(c.get_name());
            }
        }
        let rest: Vec<_> = shown(&root).filter(|c| !done.contains(&c.get_name())).collect();
        if !rest.is_empty() {
            println!("\n{}", "More".bold());
            for c in rest {
                tree(c, "cluaiz", 1);
            }
        }
    } else {
        let parent = shown_path.rsplit_once(' ').map_or("cluaiz", |(p, _)| p).to_string();
        tree(command, &parent, 0);
        println!("\n{}", format!("`{shown_path} <command> -h` for a command's options").dimmed());
    }
    Ok(())
}

/// The subcommands a user sees (not hidden ones, not clap's own `help`)
fn shown(command: &clap::Command) -> impl Iterator<Item = &clap::Command> {
    command.get_subcommands().filter(|c| !c.is_hide_set() && c.get_name() != "help")
}

/// One command (its usage, then what it does), then its subcommands one level deeper
fn tree(command: &clap::Command, parent: &str, depth: usize) {
    let path = format!("{parent} {}", command.get_name());
    let indent = "  ".repeat(depth);
    let aliases: Vec<&str> = command.get_visible_aliases().collect();
    let aliases = if aliases.is_empty() { String::new() } else { format!(" (also: {})", aliases.join(", ")) };
    println!("{indent}{}{}", usage(&path, command).green(), aliases.dimmed());
    if let Some(about) = command.get_about() {
        println!("{indent}    {}", about.to_string().dimmed());
    }
    for sub in shown(command) {
        tree(sub, &path, depth + 1);
    }
}

/// `cluaiz provider add <NAME> --url <URL> [options]`: the arguments a command needs
fn usage(path: &str, command: &clap::Command) -> String {
    let mut parts = vec![path.to_string()];
    if shown(command).next().is_some() {
        parts.push(if command.is_subcommand_required_set() { "<command>" } else { "[command]" }.to_string());
    }
    let own = |a: &&clap::Arg| !matches!(a.get_id().as_str(), "help" | "version" | "verbose") && !a.is_hide_set();
    for arg in command.get_positionals().filter(own) {
        let name = arg.get_value_names().and_then(|v| v.first()).map(|s| s.to_string()).unwrap_or_else(|| arg.get_id().as_str().to_uppercase());
        let many = matches!(arg.get_action(), clap::ArgAction::Append) || arg.get_num_args().is_some_and(|n| n.max_values() > 1);
        let dots = if many { "..." } else { "" };
        parts.push(if arg.is_required_set() { format!("<{name}>{dots}") } else { format!("[{name}]{dots}") });
    }
    let flags: Vec<&clap::Arg> = command.get_arguments().filter(|a| !a.is_positional()).filter(own).collect();
    for arg in flags.iter().filter(|a| a.is_required_set()) {
        let value = arg.get_value_names().and_then(|v| v.first()).map(|s| format!(" <{s}>")).unwrap_or_default();
        if let Some(long) = arg.get_long() {
            parts.push(format!("--{long}{value}"));
        }
    }
    if flags.iter().any(|a| !a.is_required_set()) {
        parts.push("[options]".to_string());
    }
    parts.join(" ")
}

pub fn print<C: CommandFactory>() {
    let command = C::command();
    let version = command.get_version().unwrap_or_default().to_owned();
    cluaiz_terminal::mascot::banner(&[&format!("Cluaiz {version} · run and serve local AI models")]);

    let visible: Vec<&clap::Command> = command.get_subcommands().filter(|c| !c.is_hide_set() && c.get_name() != "help").collect();
    let width = visible.iter().map(|c| c.get_name().len()).max().unwrap_or(8) + 2;
    let line = |c: &clap::Command| {
        let about = c.get_about().map(|a| a.to_string()).unwrap_or_default();
        let aliases: Vec<&str> = c.get_visible_aliases().collect();
        let aliases = if aliases.is_empty() { String::new() } else { format!(" ({})", aliases.join(", ")) };
        println!("  {}{}{}", format!("{:<width$}", c.get_name()).green(), about, aliases.dimmed());
    };

    println!("{} {} {}", "Usage:".bold(), "cluaiz".cyan(), "<command> [options]".dimmed());
    let mut shown = Vec::new();
    for (title, names) in GROUPS {
        let commands: Vec<_> = names.iter().filter_map(|name| visible.iter().find(|c| c.get_name() == *name)).collect();
        if commands.is_empty() {
            continue;
        }
        println!("\n{}", title.bold());
        for c in commands {
            line(c);
            shown.push(c.get_name());
        }
    }
    let rest: Vec<_> = visible.iter().filter(|c| !shown.contains(&c.get_name())).collect();
    if !rest.is_empty() {
        println!("\n{}", "More".bold());
        for c in rest {
            line(c);
        }
    }
    println!("\n{}", "Options".bold());
    println!("  {}{}", format!("{:<width$}", "-v, -vv").green(), "Engine logs on stderr (info / debug); `cluaiz -v` alone prints the version");
    println!("  {}{}", format!("{:<width$}", "-V").green(), "Print the version");
    println!("  {}{}", format!("{:<width$}", "-h").green(), "This help; `cluaiz <command> -h` for a command's options");
    println!("  {}{}", format!("{:<width$}", "help all").green(), "Every command and subcommand at once; `cluaiz help <command> all` for one command");
    println!(
        "\n{} {}   {}",
        "Start:".bold(),
        "cluaiz pull <model>".cyan(),
        "then  cluaiz agent  (in a project folder)  or  cluaiz run <model>".dimmed()
    );
}
