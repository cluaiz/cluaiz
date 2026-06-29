/* Theme management for cluaiz . 
   Unused fields removed to maintain global professional standards. */
use ratatui::prelude::*;

#[derive(Clone)]
pub struct Theme {
    pub tabs: Style,
    pub tabs_selected: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            tabs: Style::default().fg(Color::DarkGray),
            tabs_selected: Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        }
    }
}
