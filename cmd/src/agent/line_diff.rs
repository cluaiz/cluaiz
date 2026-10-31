//! The line diff the agent shows for a file change: removed lines `- `, added `+ `, and a few
//! unchanged lines around each change (`  `), the rest folded into one row.

/// Unchanged lines kept around a change
const CONTEXT: usize = 2;
/// Above this many line pairs the middle is shown as all removed, then all added (no table that size)
const MAX_PAIRS: usize = 4_000_000;

pub fn lines(old: &str, new: &str) -> String {
    let ops = ops(old, new);
    let changed: Vec<usize> = ops.iter().enumerate().filter(|(_, (kind, _))| *kind != ' ').map(|(i, _)| i).collect();
    if changed.is_empty() {
        return "(no change)".into();
    }
    let near = |i: usize| changed.iter().any(|&c| c.abs_diff(i) <= CONTEXT);
    let fold = |n: usize| format!("  ⋯ {n} unchanged line{}", if n == 1 { "" } else { "s" });
    let mut out = Vec::new();
    let mut skipped = 0;
    for (i, (kind, line)) in ops.iter().enumerate() {
        if *kind == ' ' && !near(i) {
            skipped += 1;
            continue;
        }
        if skipped > 0 {
            out.push(fold(skipped));
            skipped = 0;
        }
        out.push(format!("{kind} {line}"));
    }
    if skipped > 0 {
        out.push(fold(skipped));
    }
    out.join("\n")
}

/// Lines added and removed, as [`lines`] shows them
pub fn count(old: &str, new: &str) -> (usize, usize) {
    let ops = ops(old, new);
    let of = |kind| ops.iter().filter(|(k, _)| *k == kind).count();
    (of('+'), of('-'))
}

fn ops<'a>(old: &'a str, new: &'a str) -> Vec<(char, &'a str)> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // Unchanged start and end need no table
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (old_mid, new_mid) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);

    let mut ops: Vec<(char, &str)> = a[..head].iter().map(|line| (' ', *line)).collect();
    if old_mid.len().saturating_mul(new_mid.len()) <= MAX_PAIRS {
        ops.extend(common(old_mid, new_mid));
    } else {
        ops.extend(old_mid.iter().map(|line| ('-', *line)));
        ops.extend(new_mid.iter().map(|line| ('+', *line)));
    }
    ops.extend(a[a.len() - tail..].iter().map(|line| (' ', *line)));
    ops
}

/// Longest common subsequence of lines: kept lines ' ', removed '-', added '+' (removed first)
fn common<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(char, &'a str)> {
    let width = b.len() + 1;
    let mut table = vec![0u32; (a.len() + 1) * width];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i * width + j] = if a[i] == b[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut ops = Vec::with_capacity(a.len() + b.len());
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            ops.push((' ', a[i]));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            ops.push(('-', a[i]));
            i += 1;
        } else {
            ops.push(('+', b[j]));
            j += 1;
        }
    }
    ops.extend(a[i..].iter().map(|line| ('-', *line)));
    ops.extend(b[j..].iter().map(|line| ('+', *line)));
    ops
}

#[cfg(test)]
mod tests {
    use super::{count, lines};

    #[test]
    fn a_replaced_file_shows_what_went_and_what_came() {
        assert_eq!(lines("hello", "<html>\n<body>\n</html>"), "- hello\n+ <html>\n+ <body>\n+ </html>");
        assert_eq!(count("hello", "<html>\n<body>\n</html>"), (3, 1));
        assert_eq!(lines("", "a\nb"), "+ a\n+ b");
        assert_eq!(lines("a\nb", "a\nb"), "(no change)");
    }

    #[test]
    fn only_changed_lines_and_their_context_are_shown() {
        let old: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        let new = old.replace("line 10\n", "line ten\n");
        assert_eq!(
            lines(&old, &new),
            "  ⋯ 7 unchanged lines\n  line 8\n  line 9\n- line 10\n+ line ten\n  line 11\n  line 12\n  ⋯ 8 unchanged lines"
        );
        // Windows line breaks in the old file are not a change
        assert_eq!(lines("a\r\nb\r\n", "a\nb\nc\n"), "  a\n  b\n+ c");
    }
}
