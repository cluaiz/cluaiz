//! The Cluaiz mascot: its text art, its colours, and a short animated banner that every
//! interactive command prints at start (the agent's intro draws the same art in its screen).

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

/// The mascot, 15 rows of `░▒▓` text
pub const ART: &str = "              ░▒▓▓▓▓▓▓▓▓▓▒░\n            ▒▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▒\n         ▒▓▓▒░              ░▒▓░\n        ▓▓                     ▒▒\n       ▓▓     ░▓▓░      ▓▓░     ▒▒\n      ░▓▓      ▓▓       ▒▓      ▒▒░\n      ▓▓▓         ▒▓▓▓▒         ▒▒▒\n     ░▓▓▓▓          ░          ▒▒▒▒░\n     ▓▓▓▓▓▒                   ▒▓▒▒▒▒\n   ░▓▓▓▓▓▓▓▓▒░            ░░▒▒▒▒▒▒▒░\n  ▓▓▓▓▓▓▓▓▓▓▓▓▓▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒\n    ░░░░ ▒▒▒▒▒▒▒▒▒▒▒░▒▒▒▒▒▒▒▒▒░  ░░░\n          ▓▒▒▒▒▒▒░░   ░▒▒▒▒▒▒▒▒\n          ▒▓▒▒▒▒░       ▒▒▒▒▒▒░\n            ░░░          ░░░░";
/// The same mascot with its eyes closed (same rows and widths)
pub const ART_BLINK: &str = "              ░▒▓▓▓▓▓▓▓▓▓▒░\n            ▒▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▒\n         ▒▓▓▒░              ░▒▓░\n        ▓▓                     ▒▒\n       ▓▓     ───       ───     ▒▒\n      ░▓▓                       ▒▒░\n      ▓▓▓         ▒▓▓▓▒         ▒▒▒\n     ░▓▓▓▓          ░          ▒▒▒▒░\n     ▓▓▓▓▓▒                   ▒▓▒▒▒▒\n   ░▓▓▓▓▓▓▓▓▒░            ░░▒▒▒▒▒▒▒░\n  ▓▓▓▓▓▓▓▓▓▓▓▓▓▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒\n    ░░░░ ▒▒▒▒▒▒▒▒▒▒▒░▒▒▒▒▒▒▒▒▒░  ░░░\n          ▓▒▒▒▒▒▒░░   ░▒▒▒▒▒▒▒▒\n          ▒▓▒▒▒▒░       ▒▒▒▒▒▒░\n            ░░░          ░░░░";

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

/// The mascot on stderr, row by row, then one blink; `lines` follow it (dimmed). Only on a terminal
/// with colours: piped or `NO_COLOR` output gets nothing, so scripts and logs stay clean.
pub fn banner(lines: &[&str]) {
    let mut err = io::stderr();
    if !err.is_terminal() || std::env::var_os("NO_COLOR").is_some() {
        return;
    }
    let rows: Vec<&str> = ART.lines().collect();
    for row in &rows {
        let _ = writeln!(err, "{}", paint(row));
        let _ = err.flush();
        std::thread::sleep(Duration::from_millis(12));
    }
    // Blink: the eye rows only, then back
    let blink: Vec<&str> = ART_BLINK.lines().collect();
    let eyes = rows.iter().zip(&blink).enumerate().filter(|(_, (open, shut))| open != shut).map(|(i, _)| i);
    let eyes: Vec<usize> = eyes.collect();
    for (frame, wait) in [(&blink, 260), (&rows, 140)] {
        std::thread::sleep(Duration::from_millis(wait));
        for &i in &eyes {
            let up = rows.len() - i;
            let _ = write!(err, "\x1b[{up}A\r\x1b[2K{}\x1b[{up}B\r", paint(frame[i]));
        }
        let _ = err.flush();
    }
    for line in lines {
        let _ = writeln!(err, "\x1b[2m{line}\x1b[22m");
    }
    let _ = writeln!(err);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blink_keeps_every_row_width() {
        assert_eq!(ART.lines().count(), 15);
        for (open, shut) in ART.lines().zip(ART_BLINK.lines()) {
            assert_eq!(open.chars().count(), shut.chars().count(), "{open}");
        }
    }
}
