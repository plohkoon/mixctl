use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{AppState, Panel};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let panel_name = match state.active_panel {
        Panel::Routes => "Routes",
        Panel::Streams => "Streams",
        Panel::Outputs => "Output",
        Panel::Rules => "Rules",
        Panel::Capture => "Capture",
        Panel::Settings => "Settings",
    };

    let line = Line::from(vec![
        Span::styled(" Connected ", Style::default().fg(Color::Green)),
        Span::raw("│ "),
        Span::styled(format!("[{panel_name}]"), Style::default().fg(Color::Cyan)),
        Span::raw(" │ Tab: panel │ hjkl: nav │ m: mute │ ?: help │ q: quit"),
    ]);

    let bar = Paragraph::new(line)
        .style(Style::default().bg(Color::Rgb(25, 25, 30)).fg(Color::DarkGray));
    frame.render_widget(bar, area);
}
