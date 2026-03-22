use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::parse_color;
use crate::app::{AppState, Panel};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = matches!(state.active_panel, Panel::Settings);
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let mut items: Vec<ListItem> = Vec::new();
    let cursor_offset = state.inputs.len();

    // Section header: Input Colours
    items.push(ListItem::new(Line::from(Span::styled(
        "Input Colours:",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ))));

    for (i, input) in state.inputs.iter().enumerate() {
        let is_selected = is_active && state.settings_cursor == i;
        let cursor = if is_selected { "\u{25b8} " } else { "  " };
        let color = parse_color(&input.color);
        let block_str = "\u{2588}\u{2588}\u{2588}\u{2588}";

        let line = Line::from(vec![
            Span::raw(cursor),
            Span::styled(
                format!("{:<10}", input.name),
                Style::default().fg(if is_selected { Color::White } else { Color::Gray }),
            ),
            Span::styled(
                format!("{:<10}", input.color),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(block_str, Style::default().fg(color)),
        ]);

        items.push(ListItem::new(line));
    }

    // Blank line separator
    items.push(ListItem::new(Line::from("")));

    // Section header: Output Colours
    items.push(ListItem::new(Line::from(Span::styled(
        "Output Colours:",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ))));

    for (i, output) in state.outputs.iter().enumerate() {
        let is_selected = is_active && state.settings_cursor == cursor_offset + i;
        let cursor = if is_selected { "\u{25b8} " } else { "  " };
        let color = parse_color(&output.color);
        let block_str = "\u{2588}\u{2588}\u{2588}\u{2588}";

        let line = Line::from(vec![
            Span::raw(cursor),
            Span::styled(
                format!("{:<10}", output.name),
                Style::default().fg(if is_selected { Color::White } else { Color::Gray }),
            ),
            Span::styled(
                format!("{:<10}", output.color),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(block_str, Style::default().fg(color)),
        ]);

        items.push(ListItem::new(line));
    }

    // BEACN section
    if state.beacn_connected {
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(Span::styled(
            "BEACN Device (connected):",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ))));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  Layout: "),
            Span::styled("dial", Style::default().fg(Color::Gray)),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  Dial sensitivity: "),
            Span::styled("2", Style::default().fg(Color::Gray)),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  Display brightness: "),
            Span::styled("40", Style::default().fg(Color::Gray)),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  LED brightness: "),
            Span::styled("255", Style::default().fg(Color::Gray)),
        ])));
    }

    let hint = Line::from(vec![
        Span::styled("c", Style::default().fg(Color::Yellow)),
        Span::raw(": cycle colour"),
    ]);

    let list = List::new(items).block(
        block.title_bottom(hint.alignment(Alignment::Center)),
    );
    frame.render_widget(list, area);
}
