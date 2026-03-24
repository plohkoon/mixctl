use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::AppState;
use mixctl_core::config_sections::ButtonMappings;

/// Display labels for each button row (matches ButtonMappings::BUTTON_NAMES order).
const BUTTON_LABELS: &[&str] = &[
    "Dial 1", "Dial 2", "Dial 3", "Dial 4",
    "Audience 1", "Audience 2", "Audience 3", "Audience 4",
    "Mix", "Page Left", "Page Right",
];

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let border_style = Style::default().fg(Color::Magenta);

    let config = match &state.beacn_config {
        Some(c) => c,
        None => {
            let block = Block::default()
                .title(" Beacn Device ")
                .borders(Borders::ALL)
                .border_style(border_style);
            let text = Paragraph::new("  (no Beacn device connected)")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(text, area);
            return;
        }
    };

    let mappings = &config.button_mappings;

    // Build list items
    let mut items: Vec<ListItem> = Vec::new();

    // Device info header
    items.push(ListItem::new(Line::from(vec![
        Span::styled(" Layout: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&config.layout, Style::default().fg(Color::White)),
        Span::styled("    Sensitivity: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            config.dial_sensitivity.to_string(),
            Style::default().fg(Color::White),
        ),
        Span::styled("    Hold: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}ms", config.hold_threshold_ms),
            Style::default().fg(Color::White),
        ),
    ])));

    items.push(ListItem::new(Line::from(vec![
        Span::styled(" Display: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            config.display_brightness.to_string(),
            Style::default().fg(Color::White),
        ),
        Span::styled("       LEDs: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            config.led_brightness.to_string(),
            Style::default().fg(Color::White),
        ),
    ])));

    items.push(ListItem::new(Line::from("")));

    // Column headers
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            " Button Mappings:",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ])));

    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            format!(" {:<14} {:<22} {:<22}", "", "Press", "Hold"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::UNDERLINED),
        ),
    ])));

    // Button mapping rows
    for (i, name) in ButtonMappings::BUTTON_NAMES.iter().enumerate() {
        let mapping = match mappings.get(name) {
            Some(m) => m,
            None => continue,
        };

        let label = BUTTON_LABELS.get(i).copied().unwrap_or(name);
        let is_selected = state.beacn_cursor == i;
        let cursor_marker = if is_selected { "\u{25b8}" } else { " " };

        let press_name = mapping.press.display_name();
        let hold_name = mapping.hold.display_name();

        let press_field_selected = is_selected && state.beacn_field == 0;
        let hold_field_selected = is_selected && state.beacn_field == 1;

        let press_display = if state.beacn_editing && press_field_selected {
            format!("[{}]", press_name)
        } else {
            press_name.clone()
        };

        let hold_display = if state.beacn_editing && hold_field_selected {
            format!("[{}]", hold_name)
        } else {
            hold_name.clone()
        };

        let press_style = if press_field_selected {
            if state.beacn_editing {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            }
        } else if is_selected {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };

        let hold_style = if hold_field_selected {
            if state.beacn_editing {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            }
        } else if is_selected {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };

        let label_style = if is_selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{}{:<14}", cursor_marker, label),
                label_style,
            ),
            Span::raw(" "),
            Span::styled(format!("{:<22}", press_display), press_style),
            Span::styled(format!("{:<22}", hold_display), hold_style),
        ])));
    }

    // Bottom hint line
    let hint = if state.beacn_editing {
        Line::from(vec![
            Span::styled("h/l", Style::default().fg(Color::Yellow)),
            Span::raw(":cycle  "),
            Span::styled("j/k", Style::default().fg(Color::Yellow)),
            Span::raw(":nav  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(":done  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(":close"),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(":edit  "),
            Span::styled("h/l", Style::default().fg(Color::Yellow)),
            Span::raw(":field  "),
            Span::styled("j/k", Style::default().fg(Color::Yellow)),
            Span::raw(":nav  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(":close"),
        ])
    };

    let block = Block::default()
        .title(" Beacn Device ")
        .title_bottom(hint.alignment(Alignment::Center))
        .borders(Borders::ALL)
        .border_style(border_style);

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
