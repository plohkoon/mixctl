use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::parse_color;
use crate::app::{AppState, Panel};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = matches!(state.active_panel, Panel::Rules);
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" App Rules ")
        .borders(Borders::ALL)
        .border_style(border_style);

    if state.rules.is_empty() {
        let text = ratatui::widgets::Paragraph::new("  (no rules)")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = state
        .rules
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            let input_name = state
                .inputs
                .iter()
                .find(|inp| inp.id == rule.input_id)
                .map(|inp| inp.name.as_str())
                .unwrap_or("?");
            let input_color = state
                .inputs
                .iter()
                .find(|inp| inp.id == rule.input_id)
                .map(|inp| parse_color(&inp.color))
                .unwrap_or(Color::Gray);

            let is_selected = is_active && i == state.rule_cursor;
            let cursor = if is_selected { "\u{25b8} " } else { "  " };

            let line = Line::from(vec![
                Span::raw(cursor),
                Span::styled(
                    format!("{:<16}", rule.app_name),
                    Style::default().fg(if is_selected { Color::White } else { Color::Gray }),
                ),
                Span::raw(" \u{2192} "),
                Span::styled(input_name, Style::default().fg(input_color)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let hint = Line::from(vec![
        Span::styled("d", Style::default().fg(Color::Yellow)),
        Span::raw(": delete  "),
        Span::styled("1-9", Style::default().fg(Color::Yellow)),
        Span::raw(": assign to input"),
    ]);

    let list = List::new(items).block(
        block.title_bottom(hint.alignment(Alignment::Center)),
    );
    frame.render_widget(list, area);
}
