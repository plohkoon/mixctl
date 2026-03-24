mod capture;
mod dsp;
mod routes;
mod rules;
mod settings;
mod streams;
mod status;
mod tabs;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::AppState;

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    // Top-level layout: tabs | main | status
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // output tabs
            Constraint::Min(6),   // main content
            Constraint::Length(1), // status bar
        ])
        .split(area);

    tabs::render(frame, chunks[0], state);

    // Main area: split horizontally for left panel + streams (right)
    if area.width >= 80 {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(chunks[1]);

        // Left side: active left panel + output master
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(4),    // left panel
                Constraint::Length(5), // output master
            ])
            .split(main_chunks[0]);

        // Render the current left-side panel
        match state.active_panel {
            crate::app::Panel::Rules => rules::render(frame, left_chunks[0], state),
            crate::app::Panel::Capture => capture::render(frame, left_chunks[0], state),
            crate::app::Panel::Settings => settings::render(frame, left_chunks[0], state),
            crate::app::Panel::Dsp => dsp::render(frame, left_chunks[0], state),
            _ => routes::render(frame, left_chunks[0], state),
        }
        render_output_master(frame, left_chunks[1], state);
        streams::render(frame, main_chunks[1], state);
    } else {
        // Narrow terminal: show only the active panel
        match state.active_panel {
            crate::app::Panel::Routes => routes::render(frame, chunks[1], state),
            crate::app::Panel::Streams => streams::render(frame, chunks[1], state),
            crate::app::Panel::Outputs => render_output_master(frame, chunks[1], state),
            crate::app::Panel::Rules => rules::render(frame, chunks[1], state),
            crate::app::Panel::Capture => capture::render(frame, chunks[1], state),
            crate::app::Panel::Settings => settings::render(frame, chunks[1], state),
            crate::app::Panel::Dsp => dsp::render(frame, chunks[1], state),
        }
    }

    status::render(frame, chunks[2], state);

    // Help overlay
    if state.show_help {
        render_help(frame, area);
    }
}

fn render_output_master(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = matches!(state.active_panel, crate::app::Panel::Outputs);
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Output Master ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(output) = state.outputs.get(state.selected_output_idx) {
        let color = parse_color(&output.color);
        let mute_indicator = if output.muted { " M" } else { "" };
        let label = format!("{} {}%{}", output.name, output.volume, mute_indicator);

        let gauge = ratatui::widgets::Gauge::default()
            .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
            .ratio(output.volume as f64 / 100.0)
            .label(label);

        if inner.height > 0 {
            frame.render_widget(gauge, Rect { y: inner.y + inner.height / 2, height: 1, ..inner });
        }
    }
}

fn render_help(frame: &mut Frame, area: Rect) {
    let help_text = "\
 Navigation
  Tab/Shift-Tab  Switch panel
  j/k ↑/↓        Move cursor
  1-9             Select output tab
  ?               Toggle help

 Volume
  h/l ←/→         Adjust ±5
  H/L              Adjust ±1

 Mute
  m     Toggle route/output mute
  M     Toggle output master mute

 Settings
  r     Rename channel
  c     Cycle colour
  t     Cycle target device
  a     Add channel
  x     Remove channel
  J/K   Reorder up/down

 Capture
  a     Add capture input
  x     Remove capture input
  h/l   Adjust capture volume
  m     Toggle capture mute

 DSP
  Enter  Edit DSP parameters
  e/g/d  Toggle EQ/gate/de-esser
  c/l    Toggle compressor/limiter
  R      Reset EQ
  (edit) h/l H/L j/k Esc

 General
  q     Quit";

    let w = 42.min(area.width.saturating_sub(4));
    let h = 30.min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup_area = Rect::new(x, y, w, h);

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup_area);
}

pub fn parse_color(hex: &str) -> Color {
    if let Some(rgb) = mixctl_core::parse_hex_color(hex) {
        Color::Rgb(rgb.0, rgb.1, rgb.2)
    } else {
        Color::Gray
    }
}

/// Look up an input's display name and colour in a single scan.
pub(crate) fn find_input_display(inputs: &[mixctl_core::InputInfo], id: u32) -> (&str, Color) {
    inputs
        .iter()
        .find(|inp| inp.id == id)
        .map(|inp| (inp.name.as_str(), parse_color(&inp.color)))
        .unwrap_or(("?", Color::Gray))
}
