use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // If we're in the "save as" text-input mode, render that variant.
    if let Some(ref buf) = state.profile_name_buf {
        render_save_input(frame, area, buf);
        return;
    }

    // Normal profile list view.
    let hint = Line::from(vec![
        Span::styled("s", Style::default().fg(Color::Yellow)),
        Span::raw(":save  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(":load  "),
        Span::styled("x", Style::default().fg(Color::Yellow)),
        Span::raw(":delete  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(":close"),
    ]);

    let block = Block::default()
        .title(" Profiles ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title_bottom(hint.alignment(Alignment::Center));

    if state.profiles.is_empty() {
        let text = Paragraph::new("  (no saved profiles)")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = state
        .profiles
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let is_selected = i == state.profile_cursor;
            let cursor = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(vec![
                Span::raw(cursor),
                Span::styled(name, style),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_save_input(frame: &mut Frame, area: Rect, buf: &str) {
    let block = Block::default()
        .title(" Save Profile ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    let prompt = Paragraph::new(Line::from(vec![
        Span::styled("Save as: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{}\u{2588}", buf),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(
        prompt,
        Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        },
    );

    let hint = Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": confirm  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": cancel"),
    ]);
    let hint_p = Paragraph::new(hint)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(
        hint_p,
        Rect {
            y: inner.y + inner.height.saturating_sub(1),
            height: 1,
            ..inner
        },
    );
}
