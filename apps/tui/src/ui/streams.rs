use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::parse_color;
use crate::app::{AppState, Panel};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = matches!(state.active_panel, Panel::Streams);
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Streams ")
        .borders(Borders::ALL)
        .border_style(border_style);

    // Filter out internal streams
    let visible_streams: Vec<&_> = state
        .streams
        .iter()
        .filter(|s| !s.app_name.contains("mixctl.") && !s.app_name.starts_with("output."))
        .collect();

    // Bound capture devices (microphones assigned to inputs)
    let bound_captures: Vec<&_> = state
        .capture_devices
        .iter()
        .filter(|c| c.is_added && c.input_id > 0)
        .collect();

    if visible_streams.is_empty() && bound_captures.is_empty() {
        let text = ratatui::widgets::Paragraph::new("  (no streams)")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let mut items: Vec<ListItem> = Vec::new();

    // Bound capture devices first (with [mic] prefix)
    for capture in &bound_captures {
        let input_name = state
            .inputs
            .iter()
            .find(|inp| inp.id == capture.input_id)
            .map(|inp| inp.name.as_str())
            .unwrap_or("?");
        let input_color = state
            .inputs
            .iter()
            .find(|inp| inp.id == capture.input_id)
            .map(|inp| parse_color(&inp.color))
            .unwrap_or(Color::Gray);

        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled("[mic] ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{:<10}", capture.name),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(" → "),
            Span::styled(input_name, Style::default().fg(input_color)),
        ]);
        items.push(ListItem::new(line));
    }

    // App streams
    let cursor_offset = bound_captures.len();
    for (i, stream) in visible_streams.iter().enumerate() {
        let input_name = state
            .inputs
            .iter()
            .find(|inp| inp.id == stream.input_id)
            .map(|inp| inp.name.as_str())
            .unwrap_or("?");
        let input_color = state
            .inputs
            .iter()
            .find(|inp| inp.id == stream.input_id)
            .map(|inp| parse_color(&inp.color))
            .unwrap_or(Color::Gray);

        let is_selected = is_active && (i + cursor_offset) == state.stream_cursor;
        let cursor = if is_selected { "▸ " } else { "  " };

        let line = Line::from(vec![
            Span::raw(cursor),
            Span::styled(
                format!("{:<16}", stream.app_name),
                Style::default().fg(if is_selected { Color::White } else { Color::Gray }),
            ),
            Span::raw(" → "),
            Span::styled(input_name, Style::default().fg(input_color)),
        ]);
        items.push(ListItem::new(line));
    }

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
