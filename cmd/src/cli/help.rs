//! `cluaiz`, `cluaiz -h` and `cluaiz help`: the mascot banner, then the commands in groups (the
//! descriptions are clap's own, so they never drift from `cluaiz <command> --help`).

use clap::CommandFactory;
use colored::Colorize;

/// Groups in the order a new user needs them; a command missing here goes under "More"
const GROUPS: &[(&str, &[&str])] = &[
    ("Chat and agent", &["agent", "run"]),
    ("Models", &["pull", "list", "show", "rm", "model", "ps"]),
    ("Server", &["serve", "mcp-server", "token", "permission"]),
    ("This machine", &["status", "calibrate", "bench"]),
    ("Tools", &["skill", "plugin", "mcp"]),
    ("Documents and storage", &["ingest", "storage"]),
    ("Settings", &["config"]),
];

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
    println!(
        "\n{} {}   {}",
        "Start:".bold(),
        "cluaiz pull <model>".cyan(),
        "then  cluaiz agent  (in a project folder)  or  cluaiz run <model>".dimmed()
    );
}
