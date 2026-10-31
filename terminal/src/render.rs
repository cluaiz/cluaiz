//! Responsive rendering of a UI projection; only Screen writes terminal bytes.

use crate::{app::App, sanitize::display_text, theme::Theme};
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

const MIN_WIDTH: u16 = 24;
const MIN_HEIGHT: u16 = 9;
const MAX_COMPOSER_LINES: usize = 6;

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let palette = app.theme.palette();
    frame.render_widget(Block::default().style(palette.base), area);
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        app.approval_bottom = false;
        frame.render_widget(
            Paragraph::new("Terminal too small. Resize, or Ctrl+Q to exit.").style(palette.base),
            area,
        );
        return;
    }
    let input_height = app
        .composer
        .display_rows(area.width, MAX_COMPOSER_LINES)
        .clamp(1, MAX_COMPOSER_LINES) as u16
        + 2;
    let help: Vec<_> = if app.help_open {
        help_rows(area.width)
    } else {
        Vec::new()
    };
    let footer_height = if app.help_open {
        (help.len() as u16 + 1)
            .min(area.height / 3)
            .min(area.height.saturating_sub(input_height + 2))
            .max(2)
    } else {
        1
    };
    let [transcript, notice, attachments, composer, footer] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(app.attachments.len().min(3) as u16),
        Constraint::Length(input_height.min(area.height.saturating_sub(5))),
        Constraint::Length(footer_height),
    ])
    .areas(area);
    app.transcript_area = transcript;
    let rows = app.transcript.visible_animated(
        transcript.width,
        transcript.height,
        palette,
        app.activity.as_deref(),
    );
    frame.render_widget(Paragraph::new(rows).style(palette.base), transcript);
    if let Some(approval) = &app.approval {
        let width = transcript.width.saturating_sub(2).max(1) as usize;
        let lines = approval_lines(&display_text(&approval.title), &display_text(&approval.detail), width, palette);
        // The answers stay visible under the operation: 3 options and a hint
        const ANSWER_ROWS: usize = 5;
        let prompt_height = transcript
            .height
            .min((lines.len().saturating_add(2 + ANSWER_ROWS)).min(u16::MAX as usize) as u16);
        let prompt = Rect::new(
            transcript.x,
            transcript.bottom().saturating_sub(prompt_height),
            transcript.width,
            prompt_height,
        );
        let visible = (prompt.height.saturating_sub(2) as usize).saturating_sub(ANSWER_ROWS).max(1);
        app.approval_page = visible.saturating_sub(1).max(1);
        let max_scroll = lines.len().saturating_sub(visible);
        app.approval_scroll = app.approval_scroll.min(max_scroll);
        app.approval_bottom = visible > 0 && app.approval_scroll == max_scroll;
        frame.render_widget(Clear, prompt);
        frame.render_widget(
            Paragraph::new(
                lines
                    .into_iter()
                    .skip(app.approval_scroll)
                    .take(visible)
                    .collect::<Vec<_>>(),
            )
            .block(Block::bordered().border_style(palette.accent).title(if app.approval_bottom {
                " Permission required "
            } else {
                " Permission · PgDn: read more "
            }))
            .style(palette.base),
            prompt,
        );
        let inner = prompt.inner(ratatui::layout::Margin::new(1, 1));
        if inner.height as usize > ANSWER_ROWS {
            let answers_area = Rect::new(inner.x, inner.bottom() - ANSWER_ROWS as u16, inner.width, ANSWER_ROWS as u16);
            let tool = display_text(&approval.title);
            let answers = [
                "Yes, run it".to_owned(),
                format!("Yes, and do not ask again for {tool} this session"),
                "No (or type what to do instead, then Enter)".to_owned(),
            ];
            let mut rows: Vec<Line> = vec![Line::default()];
            for (index, answer) in answers.iter().enumerate() {
                let selected = index == app.approval_choice;
                rows.push(Line::styled(
                    format!("{} {}. {answer}", if selected { ">" } else { " " }, index + 1),
                    if selected { palette.accent.add_modifier(Modifier::BOLD) } else { palette.base },
                ));
            }
            rows.push(Line::styled("↑↓ choose · Enter confirm · Esc deny · PgDn read more", palette.muted));
            frame.render_widget(Paragraph::new(rows), answers_area);
        }
    } else if !app.suggestions.is_empty() {
        let count = app.suggestions.len().min(5);
        let height = (count as u16 + 2).min(transcript.height);
        let menu = Rect::new(
            transcript.x,
            transcript.bottom().saturating_sub(height),
            transcript.width,
            height,
        );
        frame.render_widget(Clear, menu);
        let start = app.selected.saturating_sub(count.saturating_sub(1));
        // Two columns: the command, then what it does (dimmed), lined up
        let width = app.suggestions.iter().map(|item| Line::from(display_text(&item.value)).width()).max().unwrap_or(0);
        let items = app
            .suggestions
            .iter()
            .enumerate()
            .skip(start)
            .take(count)
            .map(|(index, item)| {
                let selected = index == app.selected;
                let name = display_text(&item.value);
                let pad = " ".repeat(width.saturating_sub(Line::from(name.as_str()).width()) + 3);
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} {name}{pad}", if selected { ">" } else { " " }),
                        if selected { palette.accent.add_modifier(Modifier::BOLD) } else { palette.base }),
                    Span::styled(display_text(&item.description), palette.muted),
                ]))
            });
        frame.render_widget(
            List::new(items)
                .block(Block::bordered().title(" Commands · ↑↓ choose · Enter run · Esc close "))
                .style(palette.base),
            menu,
        );
    }
    let message = app.notice.as_deref().map(display_text).unwrap_or_else(|| {
        if app.transcript.omitted_entries() > 0 {
            format!(
                "{} older display entries evicted; durable history belongs to the client",
                app.transcript.omitted_entries()
            )
        } else if !app.transcript.following() {
            "Reading earlier output | Ctrl+End: latest".into()
        } else {
            String::new()
        }
    });
    frame.render_widget(Paragraph::new(message).style(palette.muted), notice);
    app.attachment_area = attachments;
    app.attachment_scroll = app.attachment_scroll.min(
        app.attachments
            .len()
            .saturating_sub(usize::from(attachments.height)),
    );
    let chips = app
        .attachments
        .iter()
        .enumerate()
        .skip(app.attachment_scroll)
        .take(usize::from(attachments.height))
        .map(|(index, label)| Line::from(format!("[{} · {}] ×", index + 1, display_text(label))))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(chips).style(palette.accent), attachments);
    app.composer.editor.set_style(palette.base);
    app.composer.editor.set_cursor_line_style(Style::default());
    app.composer
        .editor
        .set_cursor_style(palette.base.add_modifier(Modifier::REVERSED));
    app.composer.editor.set_placeholder_style(palette.muted);
    // `!command` is shell mode: the line runs as a command in the folder, not as a message
    let shell = app.approval.is_none() && app.composer.text().starts_with('!');
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(if shell { palette.accent } else { palette.muted });
    app.composer.editor.set_block(if shell {
        block.title(Span::styled(" ! shell mode · runs in the folder, the agent sees the output ", palette.accent))
    } else {
        block
    });
    frame.render_widget(&app.composer.editor, composer);
    let shortcuts = if app.approval.is_some() && area.width >= 65 {
        "↑↓ + Enter: answer | y/a/n or instructions + Enter | Esc: deny | PgUp/PgDn"
    } else if app.approval.is_some() {
        "↑↓ + Enter | Esc: deny"
    } else if app.activity.is_some() {
        "Esc: stop the answer | ?: help | Ctrl+Q: exit"
    } else if area.width < 65 {
        "?: help | Ctrl+Q: exit"
    } else {
        "?: help | Ctrl+O: thinking | F2: theme | Ctrl+Q: exit"
    };
    let shortcuts = match &app.context_usage {
        Some(usage) if app.approval.is_none() => format!("{usage} | {shortcuts}"),
        _ => shortcuts.to_owned(),
    };
    frame.render_widget(Paragraph::new(shortcuts).style(palette.muted), footer);
    if app.help_open {
        let visible = usize::from(footer.height.saturating_sub(1));
        app.help_page = visible.saturating_sub(1).max(1);
        app.help_scroll = app.help_scroll.min(help.len().saturating_sub(visible));
        frame.render_widget(
            Paragraph::new(
                help.into_iter()
                    .skip(app.help_scroll)
                    .take(visible)
                    .collect::<Vec<_>>(),
            )
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .title("Shortcuts · PgUp/PgDn · Esc: close"),
            )
            .style(palette.muted),
            footer,
        );
    }
    if let Some(choice) = &app.choice {
        let width = choice
            .options
            .iter()
            .map(|o| Line::from(display_text(&o.value)).width() + Line::from(display_text(&o.description)).width() + 8)
            .max()
            .unwrap_or(30)
            .clamp(40, usize::from(area.width.saturating_sub(4)).max(40)) as u16;
        let popup = centered(area, width, choice.options.len().min(12) as u16 + 4);
        frame.render_widget(Clear, popup);
        frame.render_widget(Block::bordered().title(format!(" {} ", display_text(&choice.title))).style(palette.base), popup);
        let inner = popup.inner(ratatui::layout::Margin::new(1, 1));
        let [choices, hint] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        let label = choice.options.iter().map(|o| Line::from(display_text(&o.value)).width()).max().unwrap_or(0);
        let start = choice.selected.saturating_sub(usize::from(choices.height).saturating_sub(1));
        let items = choice.options.iter().enumerate().skip(start).map(|(index, option)| {
            let selected = index == choice.selected;
            let name = display_text(&option.value);
            let pad = " ".repeat(label.saturating_sub(Line::from(name.as_str()).width()) + 3);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} {name}{pad}", if selected { ">" } else { " " }),
                    if selected { palette.accent.add_modifier(Modifier::BOLD) } else { palette.base }),
                Span::styled(display_text(&option.description), palette.muted),
            ]))
        });
        frame.render_widget(List::new(items), choices);
        frame.render_widget(Paragraph::new("↑↓ choose · Enter apply · Esc cancel").style(palette.muted), hint);
    }
    if let Some(picker) = &app.picker {
        let popup = centered(area, 56, 8);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::bordered()
                .title(" Theme preview ")
                .style(palette.base),
            popup,
        );
        let inner = popup.inner(ratatui::layout::Margin::new(1, 1));
        let [choices, hint] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        let items = Theme::ALL.iter().enumerate().map(|(index, theme)| {
            let selected = index == picker.selected;
            ListItem::new(format!(
                "{} {}",
                if selected { ">" } else { " " },
                theme.name()
            ))
            .style(if selected {
                palette.accent.add_modifier(Modifier::BOLD)
            } else {
                palette.base
            })
        });
        frame.render_widget(List::new(items), choices);
        let help = if inner.width < 48 {
            "Enter | Esc: cancel"
        } else {
            "Arrows: preview | Enter: apply | Esc: cancel"
        };
        frame.render_widget(Paragraph::new(help).style(palette.muted), hint);
    }
}

pub(crate) fn header(
    metadata: &crate::Metadata,
    area: Rect,
    palette: crate::theme::Palette,
    brand_override: Option<&str>,
) -> Vec<Line<'static>> {
    let mut details = vec![Line::from(display_text(&metadata.title))
        .style(palette.accent.add_modifier(Modifier::BOLD))];
    if area.height >= 14 {
        details.push(Line::from(display_text(&metadata.workspace)).style(palette.muted));
    }
    details.push(Line::from(display_text(&metadata.status)).style(palette.muted));
    // Use the caller's original art, never scale/crop it or allocate another logo asset.
    for art in [
        brand_override.unwrap_or(metadata.brand),
        metadata.compact_brand,
    ] {
        let art = display_text(art);
        let blank_line = |line: &&str| {
            line.chars()
                .all(|character| character.is_whitespace() || character == '\u{2800}')
        };
        let mut lines: Vec<_> = art
            .lines()
            .skip_while(blank_line)
            .map(str::trim_end)
            .collect();
        while lines.last().is_some_and(blank_line) {
            lines.pop();
        }
        let width = lines
            .iter()
            .map(|line| Line::from(*line).width())
            .max()
            .unwrap_or(0);
        // The art on top, the details below it: side by side, long paths pushed the art around
        let details: Vec<_> = details
            .iter()
            .flat_map(|detail| {
                textwrap::wrap(&detail.to_string(), usize::from(area.width.max(1)))
                    .into_iter()
                    .map(|part| Line::from(part.into_owned()).style(detail.style))
                    .collect::<Vec<_>>()
            })
            .collect();
        if lines.is_empty()
            || width > usize::from(area.width)
            || lines.len() + 1 + details.len() > usize::from(area.height / 2)
        {
            continue;
        }
        let mut header: Vec<Line<'static>> = lines
            .iter()
            .map(|line| brand_line(line, palette.base))
            .collect();
        header.push(Line::default());
        header.extend(details);
        return header;
    }
    details
}

fn help_rows(width: u16) -> Vec<Line<'static>> {
    const MIN_COLUMN: usize = 36;
    const COLUMN_GAP: usize = 2;
    let columns = (usize::from(width) / MIN_COLUMN).clamp(1, 3);
    let column_width = usize::from(width) / columns;
    let rows = crate::keymap::SHORTCUTS.len().div_ceil(columns);
    let mut result = Vec::new();
    for row in 0..rows {
        let cells: Vec<Vec<String>> = (0..columns)
            .map(|column| {
                crate::keymap::SHORTCUTS
                    .get(column * rows + row)
                    .map(|(key, action)| {
                        textwrap::wrap(
                            &format!("{key}  {action}"),
                            column_width.saturating_sub(COLUMN_GAP).max(1),
                        )
                        .into_iter()
                        .map(|part| part.into_owned())
                        .collect()
                    })
                    .unwrap_or_default()
            })
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(0);
        for line in 0..height {
            let mut spans = Vec::new();
            for cell in &cells {
                let text = cell.get(line).cloned().unwrap_or_default();
                let padding = column_width.saturating_sub(Line::from(text.as_str()).width());
                spans.extend([Span::raw(text), Span::raw(" ".repeat(padding))]);
            }
            result.push(Line::from(spans));
        }
    }
    result
}

/// The operation in the permission box: what it does (title), then each part under a label, so a
/// command never blends into its folder: the command highlighted, paths plain, diff lines coloured
fn approval_lines(tool: &str, detail: &str, width: usize, palette: crate::theme::Palette) -> Vec<Line<'static>> {
    const LABEL: usize = 9;
    let mut action = tool.replace('_', " ");
    if let Some(first) = action.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    let mut lines = vec![Line::from(vec![
        Span::styled(action, palette.accent.add_modifier(Modifier::BOLD)),
        Span::styled(format!("  ({tool})"), palette.muted),
    ])];
    let value_width = width.saturating_sub(LABEL).max(8);
    let mut push = |label: &str, value: &str, style: Style| {
        for (index, part) in textwrap::wrap(value, value_width).into_iter().enumerate() {
            let label = if index == 0 { label } else { "" };
            lines.push(Line::from(vec![
                Span::styled(format!("{label:<LABEL$}"), palette.muted),
                Span::styled(part.into_owned(), style),
            ]));
        }
    };
    let command = palette.base.add_modifier(Modifier::BOLD | Modifier::REVERSED);
    let path = palette.base.add_modifier(Modifier::BOLD);
    let mut rows = detail.split('\n').peekable();
    if tool == "run_command" {
        let mut first = true;
        while let Some(row) = rows.next() {
            // The last "in <folder>" row is where it runs; every row before it is the command
            if rows.peek().is_none() && !first {
                if let Some(folder) = row.strip_prefix("in ") {
                    push("Folder", folder, path);
                    break;
                }
            }
            push(if first { "Command" } else { "" }, &format!(" {row} "), command);
            first = false;
        }
        return lines;
    }
    if let Some(first) = rows.next() {
        let split = ["create ", "replace ", "edit ", "read ", "outside the working folder: "]
            .iter()
            .find_map(|verb| first.strip_prefix(verb).map(|rest| (verb.trim_end_matches([' ', ':']), rest)));
        match split {
            Some(("outside the working folder", rest)) => push("Outside", rest, path),
            Some((verb, rest)) => {
                let mut verb = verb.to_owned();
                verb[..1].make_ascii_uppercase();
                push(&verb, rest, path)
            }
            None if tool.ends_with("_file") || tool.starts_with("list") => push("Path", first, path),
            None => push("", first, path),
        }
    }
    for row in rows {
        let style = match row.chars().next() {
            Some('+') => palette.base.fg(ratatui::style::Color::Green),
            Some('-') => palette.base.fg(ratatui::style::Color::Red),
            _ => palette.base,
        };
        for part in textwrap::wrap(row, width.max(8)) {
            lines.push(Line::styled(part.into_owned(), style));
        }
    }
    lines
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [column] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas::<1>(column)[0]
}

/// One row of brand art: mascot glyphs in their own colours, anything else in the theme's text style
fn brand_line(line: &str, base: Style) -> Line<'static> {
    // Runs of one colour share a span
    let mut spans: Vec<Span<'static>> = Vec::new();
    for glyph in line.chars() {
        let style = match crate::mascot::color(glyph) {
            Some((r, g, b)) => base.fg(ratatui::style::Color::Rgb(r, g, b)),
            None => base,
        };
        match spans.last_mut() {
            Some(last) if last.style == style => last.content.to_mut().push(glyph),
            _ => spans.push(Span::styled(glyph.to_string(), style)),
        }
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::approval_lines;

    fn text(tool: &str, detail: &str) -> Vec<String> {
        let palette = crate::theme::Theme::default().palette();
        approval_lines(tool, detail, 60, palette)
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>().trim_end().to_owned())
            .collect()
    }

    #[test]
    fn approval_box_separates_the_command_from_its_folder() {
        assert_eq!(
            text("run_command", "mkdir testdir\nin /work"),
            ["Run command  (run_command)", "Command   mkdir testdir", "Folder   /work"]
        );
        assert_eq!(text("read_file", "/work/README.md"), ["Read file  (read_file)", "Path     /work/README.md"]);
        assert_eq!(text("edit_file", "edit a.rs\n- x\n+ y"), ["Edit file  (edit_file)", "Edit     a.rs", "- x", "+ y"]);
    }
}
