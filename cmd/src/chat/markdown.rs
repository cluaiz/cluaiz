//! Markdown of a streamed answer, drawn for the terminal. Text is printed as it arrives; when a line
//! ends it is redrawn formatted (headings, bold, code, lists, quotes), and a table is redrawn aligned
//! once its last row has arrived. Only used when stdout is a terminal: piped output stays the model's
//! own Markdown.

use std::io::Write;

use crossterm::{cursor, queue, terminal};
use unicode_width::UnicodeWidthStr;

/// Screen adapter over the existing inline/table formatter. No terminal writes or ANSI escapes.
pub fn display_lines(source: &str, width: usize) -> Vec<String> {
    let mut format = MarkdownStream::new();
    format.color = false;
    let mut result = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !format.code && trimmed.starts_with('|') { format.table.push(line.to_owned()); continue; }
        if !format.table.is_empty() {
            let table = std::mem::take(&mut format.table);
            result.extend(format.render_table(&table, width).lines().map(str::to_owned));
        }
        if let Some(language) = trimmed.strip_prefix("```") {
            format.code = !format.code;
            if format.code && !language.trim().is_empty() { result.push(format!("  {}", language.trim())); }
        } else if format.code { result.push(format!("  │ {line}")); }
        else { result.push(format.render_line(line, width)); }
    }
    if !format.table.is_empty() { result.extend(format.render_table(&format.table, width).lines().map(str::to_owned)); }
    result
}

pub struct MarkdownStream {
    color: bool,
    /// Raw text of the line being streamed (already on screen)
    line: String,
    /// Table lines shown raw so far, redrawn aligned when the table ends
    table: Vec<String>,
    table_rows: usize,
    /// Inside a ``` block
    code: bool,
}

impl MarkdownStream {
    pub fn new() -> Self {
        Self { color: colored::control::SHOULD_COLORIZE.should_colorize(), line: String::new(), table: Vec::new(), table_rows: 0, code: false }
    }

    pub fn push(&mut self, text: &str) {
        for piece in text.split_inclusive('\n') {
            let (part, ends) = match piece.strip_suffix('\n') {
                Some(part) => (part.strip_suffix('\r').unwrap_or(part), true),
                None => (piece, false),
            };
            let part = part.replace('\t', "    ");
            write(&part);
            self.line.push_str(&part);
            if ends {
                self.end_line();
            }
        }
    }

    /// Ends the line being streamed here (drawn formatted), so other output can take the next line.
    pub fn break_line(&mut self) {
        if !self.line.is_empty() {
            self.end_line();
        }
    }

    /// Draws what is still pending; the answer then ends on a fresh line.
    pub fn finish(&mut self) {
        if !self.line.is_empty() {
            self.end_line();
        }
        self.flush_table();
        self.code = false;
    }

    fn end_line(&mut self) {
        let line = std::mem::take(&mut self.line);
        let (cols, height) = size();
        let up = rows(&line, cols) - 1;
        // A line taller than the screen cannot be redrawn: it stays as streamed
        if up + 1 >= height {
            write("\n");
            return;
        }
        let mut out = std::io::stdout().lock();
        let _ = queue!(out, cursor::MoveToColumn(0));
        if up > 0 {
            let _ = queue!(out, cursor::MoveUp(up as u16));
        }
        let _ = queue!(out, terminal::Clear(terminal::ClearType::FromCursorDown));
        let _ = out.flush();
        drop(out);

        let trimmed = line.trim_start();
        if self.code {
            if trimmed.starts_with("```") {
                self.code = false;
            } else {
                write(&format!("{}{}\n", self.paint("  │ ", "2", "22"), line));
            }
            return;
        }
        if trimmed.starts_with('|') {
            write(&format!("{}\n", line));
            self.table_rows += rows(&line, cols);
            self.table.push(line);
            return;
        }
        self.flush_table();
        if let Some(lang) = trimmed.strip_prefix("```") {
            self.code = true;
            if !lang.trim().is_empty() {
                write(&format!("{}\n", self.paint(&format!("  {}", lang.trim()), "2", "22")));
            }
            return;
        }
        write(&format!("{}\n", self.render_line(&line, cols)));
    }

    fn flush_table(&mut self) {
        if self.table.is_empty() {
            return;
        }
        let lines = std::mem::take(&mut self.table);
        let shown = std::mem::take(&mut self.table_rows);
        let (cols, height) = size();
        if shown >= height {
            return;
        }
        let mut out = std::io::stdout().lock();
        let _ = queue!(out, cursor::MoveToColumn(0), cursor::MoveUp(shown as u16), terminal::Clear(terminal::ClearType::FromCursorDown));
        let _ = out.flush();
        drop(out);
        write(&self.render_table(&lines, cols));
    }

    fn render_line(&self, line: &str, cols: usize) -> String {
        let indent = &line[..line.len() - line.trim_start().len()];
        let body = line.trim_start();

        let hashes = body.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && body[hashes..].starts_with(' ') {
            let text = self.inline(body[hashes..].trim());
            return if hashes <= 2 { self.paint(&text, "1;36", "22;39") } else { self.paint(&text, "1", "22") };
        }
        let rule = body.trim_end();
        if rule.len() >= 3 && (rule.chars().all(|c| c == '-') || rule.chars().all(|c| c == '*') || rule.chars().all(|c| c == '_')) {
            return self.paint(&"─".repeat(cols.clamp(3, 60)), "2", "22");
        }
        if let Some(quote) = body.strip_prefix('>') {
            return format!("{}{}{}", indent, self.paint("│ ", "2", "22"), self.paint(&self.inline(quote.trim_start()), "3", "23"));
        }
        for bullet in ["- ", "* ", "+ "] {
            if let Some(item) = body.strip_prefix(bullet) {
                let (mark, item) = match item {
                    i if i.starts_with("[ ] ") => ("☐ ", &i[4..]),
                    i if i.starts_with("[x] ") || i.starts_with("[X] ") => ("☑ ", &i[4..]),
                    i => ("• ", i),
                };
                return format!("{}{}{}", indent, self.paint(mark, "36", "39"), self.inline(item.trim_start()));
            }
        }
        format!("{}{}", indent, self.inline(body))
    }

    /// Bold, italic, strikethrough, `code` and [links](url) of one line, markers removed.
    fn inline(&self, s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let rest = &chars[i..];
            if c == '\\' && i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            // $math$ / $$math$$ (pandoc's rule keeps prices: "$5 and $10" is not math)
            if c == '$' {
                let marker = if starts(rest, "$$") { "$$" } else { "$" };
                let from = i + marker.len();
                if chars.get(from).is_some_and(|n| !n.is_whitespace()) {
                    if let Some(end) = find(&chars, from, marker)
                        .filter(|e| *e > from && !chars[e - 1].is_whitespace() && !chars.get(e + marker.len()).is_some_and(|n| n.is_ascii_digit()))
                    {
                        let tex: String = chars[from..end].iter().collect();
                        out.push_str(&self.paint(&latex(&tex), "3", "23"));
                        i = end + marker.len();
                        continue;
                    }
                }
            }
            if c == '`' {
                if let Some(end) = find(&chars, i + 1, "`") {
                    let code: String = chars[i + 1..end].iter().collect();
                    out.push_str(&self.paint(&code, "36", "39"));
                    i = end + 1;
                    continue;
                }
            }
            if let Some((marker, on, off)) = [("**", "1", "22"), ("__", "1", "22"), ("~~", "9", "29")].into_iter().find(|(m, _, _)| starts(rest, m)) {
                if let Some(end) = find(&chars, i + 2, marker).filter(|e| *e > i + 2) {
                    let inner: String = chars[i + 2..end].iter().collect();
                    out.push_str(&self.paint(&self.inline(&inner), on, off));
                    i = end + 2;
                    continue;
                }
            }
            // *italic* / _italic_; a marker inside a word (snake_case, 2*3*4) stays
            if (c == '*' || c == '_')
                && (i == 0 || !chars[i - 1].is_alphanumeric())
                && chars.get(i + 1).is_some_and(|n| !n.is_whitespace())
            {
                let marker = if c == '*' { "*" } else { "_" };
                if let Some(end) = find(&chars, i + 1, marker).filter(|e| *e > i + 1 && !chars[e - 1].is_whitespace()) {
                    if chars.get(end + 1).is_none_or(|n| !n.is_alphanumeric()) {
                        let inner: String = chars[i + 1..end].iter().collect();
                        out.push_str(&self.paint(&self.inline(&inner), "3", "23"));
                        i = end + 1;
                        continue;
                    }
                }
            }
            if c == '[' {
                if let Some(close) = find(&chars, i + 1, "](") {
                    if let Some(end) = find(&chars, close + 2, ")") {
                        let text: String = chars[i + 1..close].iter().collect();
                        let url: String = chars[close + 2..end].iter().collect();
                        out.push_str(&self.paint(&self.inline(&text), "4", "24"));
                        if url != text {
                            out.push_str(&self.paint(&format!(" ({})", url), "2", "22"));
                        }
                        i = end + 1;
                        continue;
                    }
                }
            }
            out.push(c);
            i += 1;
        }
        out
    }

    fn render_table(&self, lines: &[String], cols: usize) -> String {
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut aligns: Vec<char> = Vec::new();
        let mut header = None;
        for line in lines {
            let cells = split_cells(line);
            let is_rule = !cells.is_empty()
                && cells.iter().all(|c| !c.is_empty() && c.trim_matches(':').chars().all(|ch| ch == '-') && c.contains('-'));
            if is_rule && header.is_none() && rows.len() == 1 {
                aligns = cells
                    .iter()
                    .map(|c| match (c.starts_with(':'), c.ends_with(':')) {
                        (true, true) => 'c',
                        (false, true) => 'r',
                        _ => 'l',
                    })
                    .collect();
                header = Some(0);
                continue;
            }
            rows.push(cells.iter().map(|c| self.inline(c)).collect());
        }
        let n = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut widths = vec![0usize; n];
        for row in &rows {
            for (k, cell) in row.iter().enumerate() {
                widths[k] = widths[k].max(visible_width(cell));
            }
        }
        let bar = self.paint("│", "2", "22");
        let total: usize = widths.iter().sum::<usize>() + 3 * n.saturating_sub(1);
        let mut out = String::new();
        for (r, row) in rows.iter().enumerate() {
            if total > cols {
                // Too wide to align: one row per line, cells separated by bars
                let line = row.join(&format!(" {} ", bar));
                out.push_str(&if header == Some(r) { self.paint(&line, "1", "22") } else { line });
            } else {
                let cells: Vec<String> = (0..n)
                    .map(|k| {
                        let cell = row.get(k).map(String::as_str).unwrap_or("");
                        let pad = widths[k] - visible_width(cell);
                        let (left, right) = match aligns.get(k) {
                            Some('r') => (pad, 0),
                            Some('c') => (pad / 2, pad - pad / 2),
                            _ => (0, pad),
                        };
                        let cell = if header == Some(r) { self.paint(cell, "1", "22") } else { cell.to_string() };
                        format!("{}{}{}", " ".repeat(left), cell, " ".repeat(right))
                    })
                    .collect();
                out.push_str(cells.join(&format!(" {} ", bar)).trim_end());
            }
            out.push('\n');
            if header == Some(r) && total <= cols {
                let rule: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
                out.push_str(&self.paint(&rule.join("─┼─"), "2", "22"));
                out.push('\n');
            }
        }
        out
    }

    /// `text` between the SGR codes `on` and `off` (no codes without colour)
    fn paint(&self, text: &str, on: &str, off: &str) -> String {
        if self.color {
            format!("\x1b[{}m{}\x1b[{}m", on, text, off)
        } else {
            text.to_string()
        }
    }
}

fn write(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn size() -> (usize, usize) {
    let (c, r) = terminal::size().unwrap_or((80, 24));
    ((c as usize).max(1), (r as usize).max(1))
}

/// Terminal rows a line of raw text takes (a line exactly as wide as the screen takes one).
fn rows(line: &str, cols: usize) -> usize {
    let w = line.width();
    if w == 0 { 1 } else { (w - 1) / cols + 1 }
}

/// Width on screen of text with SGR colour codes.
fn visible_width(s: &str) -> usize {
    let mut plain = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for e in chars.by_ref() {
                if e.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain.width()
}

/// Common LaTeX of a math span as plain symbols; other commands stay as written.
fn latex(tex: &str) -> String {
    let mut s = tex.to_string();
    // \frac{a}{b} -> a/b, \text{x} / \mathrm{x} -> x
    while let Some(at) = s.find("\\frac{") {
        let parts = (|| {
            let a_end = at + 6 + s[at + 6..].find('}')?;
            let b_start = a_end + 1;
            if !s[b_start..].starts_with('{') {
                return None;
            }
            let b_end = b_start + s[b_start..].find('}')?;
            Some((s[at + 6..a_end].to_string(), s[b_start + 1..b_end].to_string(), b_end))
        })();
        match parts {
            Some((a, b, end)) => s.replace_range(at..=end, &format!("{}/{}", a, b)),
            None => break,
        }
    }
    for cmd in ["\\text{", "\\mathrm{", "\\mathbf{", "\\textbf{"] {
        while let Some(at) = s.find(cmd) {
            match s[at + cmd.len()..].find('}') {
                Some(len) => {
                    let inner = s[at + cmd.len()..at + cmd.len() + len].to_string();
                    s.replace_range(at..at + cmd.len() + len + 1, &inner);
                }
                None => break,
            }
        }
    }
    // Longer commands first (\rightarrow before \right, \left before \le)
    for (from, to) in [
        ("\\rightarrow", "→"), ("\\left", ""), ("\\right", ""), ("\\times", "×"), ("\\div", "÷"), ("\\cdot", "·"),
        ("\\pm", "±"), ("\\leq", "≤"), ("\\geq", "≥"), ("\\le", "≤"), ("\\ge", "≥"), ("\\neq", "≠"),
        ("\\approx", "≈"), ("\\infty", "∞"), ("\\sqrt", "√"), ("\\pi", "π"), ("\\to", "→"), ("\\,", " "),
        ("\\;", " "), ("\\quad", "  "),
    ] {
        s = s.replace(from, to);
    }
    s
}

fn starts(chars: &[char], marker: &str) -> bool {
    let m: Vec<char> = marker.chars().collect();
    chars.len() >= m.len() && chars[..m.len()] == m[..]
}

/// Index of the next `marker` at or after `from`
fn find(chars: &[char], from: usize, marker: &str) -> Option<usize> {
    (from..chars.len()).find(|&k| starts(&chars[k..], marker))
}

/// Cells of a `| a | b |` row; `\|` is a bar inside a cell.
fn split_cells(line: &str) -> Vec<String> {
    let body = line.trim();
    let body = body.strip_prefix('|').unwrap_or(body);
    let body = body.strip_suffix('|').unwrap_or(body);
    let mut cells = vec![String::new()];
    let mut escaped = false;
    for c in body.chars() {
        match c {
            '|' if !escaped => cells.push(String::new()),
            '\\' if !escaped => escaped = true,
            _ => {
                if escaped && c != '|' {
                    cells.last_mut().unwrap().push('\\');
                }
                escaped = false;
                cells.last_mut().unwrap().push(c);
            }
        }
    }
    cells.into_iter().map(|c| c.trim().to_string()).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn screen_layout_reuses_markdown_without_ansi_or_changing_code() {
        let rows = super::display_lines("# Heading\n- **important**\n```rust\nlet text = \"**literal**\";\n```", 80);
        assert!(rows.iter().any(|line| line == "Heading"));
        assert!(rows.iter().any(|line| line.contains("• important")));
        assert!(rows.iter().any(|line| line.contains("**literal**")));
        assert!(rows.iter().all(|line| !line.contains('\x1b')));
    }
    use super::*;

    fn plain() -> MarkdownStream {
        MarkdownStream { color: false, ..MarkdownStream::new() }
    }

    #[test]
    fn markers_are_removed_from_a_line() {
        let md = plain();
        assert_eq!(md.render_line("## 3 × 3 = **45**", 80), "3 × 3 = 45");
        assert_eq!(md.render_line("- use `cargo` and *care*", 80), "• use cargo and care");
        assert_eq!(md.render_line("see [docs](https://x.y) now", 80), "see docs (https://x.y) now");
        assert_eq!(md.render_line("keep snake_case_name and 2*3*4", 80), "keep snake_case_name and 2*3*4");
        assert_eq!(md.render_line(r"a \*literal\* star", 80), "a *literal* star");
        assert_eq!(md.render_line(r"$3 \times 3 + \frac{2}{4} = 45.5$ costs $5 and $10", 80), "3 × 3 + 2/4 = 45.5 costs $5 and $10");
        assert_eq!(md.render_line(r"$$\left(a \le b\right)$$", 80), "(a ≤ b)");
    }

    #[test]
    fn a_table_is_aligned() {
        let md = plain();
        let lines: Vec<String> = ["| Step | Operation |", "|---|---:|", "| 1 | **45** |", "| 2 | 0.5 |"].iter().map(|s| s.to_string()).collect();
        let table = md.render_table(&lines, 80);
        assert_eq!(table, "Step │ Operation\n─────┼──────────\n1    │        45\n2    │       0.5\n");
    }
}
