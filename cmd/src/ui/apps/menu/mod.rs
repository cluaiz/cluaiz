use ratatui::{
    layout::Rect,
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
    buffer::Buffer,
};
use crate::core::state::AppState;
use crate::theme::Theme;

pub fn render_widget(app: &mut AppState, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let lm = 2u16;
    let view_width = area.width.saturating_sub(lm * 2);
    let mut current_y = area.y;

    // ── 2. MENU OPTIONS (Native Flow) ─────────────────────────────────
    let menu_items = vec!(
        ("Model List", "Download and select backend Archer models."),
        ("Settings", "System config, API keys, Storage paths."),
        ("Help/Docs", "Read the documentation on how to use cluaiz OS."),
    );

    let selected = app.menu_state.selected().unwrap_or(0);

    for (idx, (title, desc)) in menu_items.iter().enumerate() {
        let is_sel = idx == selected;
        let style = if is_sel { theme.tabs_selected } else { Style::default().fg(Color::White) };
        
        let marker = if is_sel { "❯ " } else { "  " };

        let item = Line::from(vec![
            Span::styled(marker, if is_sel { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) }),
            Span::styled(format!("{:<15}", title), style.add_modifier(Modifier::BOLD)),
            Span::styled(format!(" │ {}", desc), Style::default().fg(Color::DarkGray)),
        ]);

        Paragraph::new(item).render(Rect::new(area.x + lm, current_y, view_width, 1), buf);
        current_y += 1;
        
    }
}
