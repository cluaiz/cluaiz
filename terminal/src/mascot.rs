//! The Cluaiz wordmark (">_ CLUAIZ"): its text art, its colours, and the short banner that every
//! interactive command prints at start (the agent's intro draws the same wordmark in its screen).

use std::io::{self, IsTerminal, Write};

/// The ">_ CLUAIZ" wordmark
pub const WORDMARK: &str = "░▀▄░░░░░░░░█▀▀░█░░░█░█░█▀█░▀█▀░▀▀█\n░░▄▀░░░░░░░█░░░█░░░█░█░█▀█░░█░░▄▀░\n░▀░░░▀▀▀░░░▀▀▀░▀▀▀░▀▀▀░▀░▀░▀▀▀░▀▀▀";

/// The colour of one glyph of the art: bright face, lighter body, dim shadow
pub fn color(glyph: char) -> Option<(u8, u8, u8)> {
    match glyph {
        '▓' => Some((249, 250, 253)),
        '▒' => Some((192, 202, 218)),
        '░' => Some((126, 148, 169)),
        '─' => Some((206, 221, 234)),
        _ => None,
    }
}

/// One row of the art with its colours as ANSI truecolor codes
fn paint(row: &str) -> String {
    let mut out = String::with_capacity(row.len() * 4);
    let mut current = None;
    for glyph in row.chars() {
        let wanted = color(glyph);
        if wanted != current {
            match wanted {
                Some((r, g, b)) => out.push_str(&format!("\x1b[38;2;{r};{g};{b}m")),
                None => out.push_str("\x1b[39m"),
            }
            current = wanted;
        }
        out.push(glyph);
    }
    out.push_str("\x1b[39m");
    out
}

/// The wordmark on stderr, then `lines` (dimmed). Only on a terminal with colours: piped or `NO_COLOR` output
/// gets nothing, so scripts and logs stay clean.
pub fn banner(lines: &[&str]) {
    let mut err = io::stderr();
    if !err.is_terminal() || std::env::var_os("NO_COLOR").is_some() {
        return;
    }
    for row in WORDMARK.lines() {
        let _ = writeln!(err, "{}", paint(row));
    }
    for line in lines {
        let _ = writeln!(err, "\x1b[2m{line}\x1b[22m");
    }
    let _ = writeln!(err);
}
