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

        let name_display = if is_selected {
            if let Some(ref buf) = state.rename_buf {
                format!("{}\u{2588}", buf)
            } else {
                format!("{:<10}", input.name)
            }
        } else {
            format!("{:<10}", input.name)
        };

        let name_style = if is_selected && state.rename_buf.is_some() {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED)
        } else if is_selected {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };

        let line = Line::from(vec![
            Span::raw(cursor),
            Span::styled(name_display, name_style),
            Span::raw(" "),
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

        let name_display = if is_selected {
            if let Some(ref buf) = state.rename_buf {
                format!("{}\u{2588}", buf)
            } else {
                format!("{:<10}", output.name)
            }
        } else {
            format!("{:<10}", output.name)
        };

        let name_style = if is_selected && state.rename_buf.is_some() {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED)
        } else if is_selected {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };

        let target_display = if output.target_device.is_empty() {
            "(default)".to_string()
        } else {
            output.target_device.clone()
        };

        let line = Line::from(vec![
            Span::raw(cursor),
            Span::styled(name_display, name_style),
            Span::raw(" "),
            Span::styled(
                format!("{:<10}", output.color),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(block_str, Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(
                format!("\u{2192} {}", target_display),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        items.push(ListItem::new(line));
    }

    // BEACN section
    if state.beacn_connected {
        let beacn = state.beacn_config.as_ref();
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(Span::styled(
            "BEACN Device (connected):",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ))));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  Layout: "),
            Span::styled(
                beacn.map(|c| c.layout.as_str()).unwrap_or("?"),
                Style::default().fg(Color::Gray),
            ),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  Dial sensitivity: "),
            Span::styled(
                beacn.map(|c| c.dial_sensitivity.to_string()).unwrap_or_else(|| "?".into()),
                Style::default().fg(Color::Gray),
            ),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  Display brightness: "),
            Span::styled(
                beacn.map(|c| c.display_brightness.to_string()).unwrap_or_else(|| "?".into()),
                Style::default().fg(Color::Gray),
            ),
        ])));
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  LED brightness: "),
            Span::styled(
                beacn.map(|c| c.led_brightness.to_string()).unwrap_or_else(|| "?".into()),
                Style::default().fg(Color::Gray),
            ),
        ])));
    }

    let hint = if state.rename_buf.is_some() {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": confirm  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": cancel"),
        ])
    } else {
        Line::from(vec![
            Span::styled("r", Style::default().fg(Color::Yellow)),
            Span::raw(": rename  "),
            Span::styled("c", Style::default().fg(Color::Yellow)),
            Span::raw(": colour  "),
            Span::styled("t", Style::default().fg(Color::Yellow)),
            Span::raw(": target device  "),
            Span::styled("a", Style::default().fg(Color::Yellow)),
            Span::raw(": add  "),
            Span::styled("x", Style::default().fg(Color::Yellow)),
            Span::raw(": remove  "),
            Span::styled("J/K", Style::default().fg(Color::Yellow)),
            Span::raw(": reorder"),
        ])
    };

    let list = List::new(items).block(
        block.title_bottom(hint.alignment(Alignment::Center)),
    );
    frame.render_widget(list, area);
}
