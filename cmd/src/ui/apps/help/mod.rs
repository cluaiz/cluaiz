use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Widget},
};
use crate::core::state::AppState;
use crate::theme::Theme;
use crate::cli::help::load_commands;

// ──────────────────────────────────────────────────────────────────────────

pub fn render_widget(app: &mut AppState, _theme: &Theme, area: Rect, buf: &mut Buffer) {
    let commands = load_commands();

    // Outer chrome
    let block = Block::default()
        .title(Span::styled(
            " 📚 cluaiz Help — Command Reference ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    block.render(area, buf);

    // Split: header + command list + footer tip
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // command list
            Constraint::Length(2), // footer tip
        ])
        .split(inner);

    // ── Header ─────────────────────────────────────────────────────────────
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  USAGE  ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("cluaiz [COMMAND]", Style::default().fg(Color::White).add_modifier(Modifier::DIM)),
        ]),
        Line::raw(""),
    ]);
    header.render(chunks[0], buf);

    // ── Command List ────────────────────────────────────────────────────────
    let categories: &[(&str, &str, Color)] = &[
        ("core",   "  Core",   Color::Cyan),
        ("models", "  Models", Color::Green),
        ("skills", "  Skills", Color::Blue),
        ("extensions", "  Extensions", Color::Magenta),
        ("system", "  System", Color::Yellow),
    ];

    let mut items: Vec<ListItem> = Vec::new();

    for (cat_key, cat_label, cat_color) in categories {
        let group: Vec<_> = commands
            .iter()
            .filter(|c| c.category.as_str() == *cat_key)
            .collect();

        if group.is_empty() {
            continue;
        }

        // Category header
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!(" {} ─────────────────────────────", cat_label),
                Style::default()
                    .fg(*cat_color)
                    .add_modifier(Modifier::BOLD | Modifier::DIM),
            ),
        ])));

        for cmd in group {
            // Command row: usage + description
            items.push(ListItem::new(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    format!("{:<35}", cmd.usage),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    cmd.description.clone(),
                    Style::default().fg(Color::Gray),
                ),
            ])));

            // Example row
            items.push(ListItem::new(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    format!("{:<35}", ""),
                    Style::default(),
                ),
                Span::styled(
                    format!("e.g. {}", cmd.example),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ),
            ])));
        }

        // Spacer
        items.push(ListItem::new(Line::raw("")));
    }

    let list = List::new(items)
        .style(Style::default().fg(Color::White));

    list.render(chunks[1], buf);

    // ── Footer Tip ──────────────────────────────────────────────────────────
    let tip = Line::from(vec![
        Span::raw("  "),
        Span::styled("💡 Tip: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            "Add commands to ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "commands.json",
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(
            " — no recompile needed.",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    Paragraph::new(tip).render(chunks[2], buf);

    // suppress unused warning for app
    let _ = app;
}
