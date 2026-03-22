use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::parse_color;
use crate::app::{AppState, Panel};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = matches!(state.active_panel, Panel::Capture);
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Capture Devices ")
        .borders(Borders::ALL)
        .border_style(border_style);

    if state.capture_devices.is_empty() {
        let text = ratatui::widgets::Paragraph::new("  (no devices)")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = state
        .capture_devices
        .iter()
        .enumerate()
        .map(|(i, device)| {
            let is_selected = is_active && i == state.capture_cursor;
            let cursor = if is_selected { "\u{25b8} " } else { "  " };

            let bound_spans = if device.input_id > 0 {
                let input_name = state
                    .inputs
                    .iter()
                    .find(|inp| inp.id == device.input_id)
                    .map(|inp| inp.name.as_str())
                    .unwrap_or("?");
                let input_color = state
                    .inputs
                    .iter()
                    .find(|inp| inp.id == device.input_id)
                    .map(|inp| parse_color(&inp.color))
                    .unwrap_or(Color::Gray);
                vec![
                    Span::raw(" \u{2192} "),
                    Span::styled(input_name, Style::default().fg(input_color)),
                ]
            } else {
                vec![
                    Span::styled(" (available)", Style::default().fg(Color::DarkGray)),
                ]
            };

            let mut spans = vec![
                Span::raw(cursor),
                Span::styled(
                    format!("{:<16}", device.name),
                    Style::default().fg(if is_selected { Color::White } else { Color::Gray }),
                ),
            ];
            spans.extend(bound_spans);

            ListItem::new(Line::from(spans))
        })
        .collect();

    let hint = Line::from(vec![
        Span::styled("1-9", Style::default().fg(Color::Yellow)),
        Span::raw(": bind  "),
        Span::styled("u", Style::default().fg(Color::Yellow)),
        Span::raw(": unbind"),
    ]);

    let list = List::new(items).block(
        block.title_bottom(hint.alignment(Alignment::Center)),
    );
    frame.render_widget(list, area);
}
