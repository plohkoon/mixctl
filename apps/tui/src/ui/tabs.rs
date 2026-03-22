use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Tabs as RataTabs};

use super::parse_color;
use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let titles: Vec<String> = state
        .outputs
        .iter()
        .enumerate()
        .map(|(i, o)| format!("[{}] {}", i + 1, o.name))
        .collect();

    let tabs = RataTabs::new(titles)
        .block(
            Block::default()
                .title(" mixctl ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .select(state.selected_output_idx)
        .highlight_style(Style::default().bold().fg(
            state
                .outputs
                .get(state.selected_output_idx)
                .map(|o| parse_color(&o.color))
                .unwrap_or(Color::White),
        ))
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(tabs, area);
}
