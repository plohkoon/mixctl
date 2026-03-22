use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge};

use super::parse_color;
use crate::app::{AppState, Panel};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = matches!(state.active_panel, Panel::Routes);
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = if let Some(output) = state.outputs.get(state.selected_output_idx) {
        format!(" Routes ({}) ", output.name)
    } else {
        " Routes ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.routes.is_empty() {
        return;
    }

    // One row per route
    let row_height = 2u16; // gauge + spacing
    let max_rows = (inner.height / row_height) as usize;

    for (i, route) in state.routes.iter().enumerate().take(max_rows) {
        let y = inner.y + (i as u16) * row_height;
        if y >= inner.y + inner.height {
            break;
        }

        // Find the input name and color for this route
        let (input_name, input_color) = state
            .inputs
            .iter()
            .find(|inp| inp.id == route.input_id)
            .map(|inp| (inp.name.as_str(), parse_color(&inp.color)))
            .unwrap_or(("?", Color::Gray));

        let is_selected = is_active && i == state.route_cursor;
        let cursor = if is_selected { "▸ " } else { "  " };
        let mute_indicator = if route.muted { " M" } else { "" };
        let label = format!("{cursor}{input_name} {}{mute_indicator}", route.volume);

        let gauge_color = if route.muted {
            Color::DarkGray
        } else {
            input_color
        };

        let gauge_style = if is_selected {
            Style::default().fg(gauge_color).bg(Color::Rgb(40, 40, 45))
        } else {
            Style::default().fg(gauge_color).bg(Color::Rgb(25, 25, 30))
        };

        let gauge = Gauge::default()
            .gauge_style(gauge_style)
            .ratio(route.volume as f64 / 100.0)
            .label(Span::styled(
                label,
                Style::default().fg(if is_selected { Color::White } else { Color::Gray }),
            ));

        let row_area = Rect::new(inner.x, y, inner.width, 1);
        frame.render_widget(gauge, row_area);
    }
}
