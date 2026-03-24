use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::parse_color;
use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // Look up default input/output names
    let default_in_name = state
        .inputs
        .iter()
        .find(|i| i.id == state.default_input)
        .map(|i| i.name.as_str())
        .unwrap_or("none");

    let default_out_name = state
        .outputs
        .iter()
        .find(|o| o.id == state.default_output)
        .map(|o| o.name.as_str())
        .unwrap_or("none");

    // Color the default names using their channel color
    let in_color = state
        .inputs
        .iter()
        .find(|i| i.id == state.default_input)
        .map(|i| parse_color(&i.color))
        .unwrap_or(Color::DarkGray);

    let out_color = state
        .outputs
        .iter()
        .find(|o| o.id == state.default_output)
        .map(|o| parse_color(&o.color))
        .unwrap_or(Color::DarkGray);

    let line = Line::from(vec![
        Span::styled(" MixCtl", Style::default().fg(Color::White).bold()),
        Span::styled(" \u{2500}\u{2500}\u{2500}\u{2500} ", Style::default().fg(Color::DarkGray)),
        Span::styled("[connected]", Style::default().fg(Color::Green)),
        Span::styled(" \u{2500}\u{2500} ", Style::default().fg(Color::DarkGray)),
        Span::styled("In: ", Style::default().fg(Color::DarkGray)),
        Span::styled(default_in_name, Style::default().fg(in_color)),
        Span::styled(" \u{2500}\u{2500} ", Style::default().fg(Color::DarkGray)),
        Span::styled("Out: ", Style::default().fg(Color::DarkGray)),
        Span::styled(default_out_name, Style::default().fg(out_color)),
    ]);

    let bar = Paragraph::new(line)
        .style(Style::default().bg(Color::Rgb(20, 20, 25)));
    frame.render_widget(bar, area);
}
