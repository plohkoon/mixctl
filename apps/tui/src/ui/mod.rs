mod dsp;
mod footer;
mod header;
mod matrix;
mod profiles;
mod settings;
mod status;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{AppState, Overlay};

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    // Top-level layout: header(1) | matrix(fill) | footer(5) | status(1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // header
            Constraint::Min(4),    // matrix grid
            Constraint::Length(5), // footer
            Constraint::Length(1), // status bar
        ])
        .split(area);

    header::render(frame, chunks[0], state);
    matrix::render(frame, chunks[1], state);
    footer::render(frame, chunks[2], state);
    status::render(frame, chunks[3], state);

    // Overlay rendering
    match state.overlay {
        Overlay::None => {}
        Overlay::Help => render_help(frame, area),
        Overlay::Dsp => render_overlay(frame, area, |f, a| dsp::render(f, a, state)),
        Overlay::Settings => render_overlay(frame, area, |f, a| settings::render(f, a, state)),
        Overlay::Profiles => render_overlay(frame, area, |f, a| profiles::render(f, a, state)),
    }
}

/// Render a full-screen overlay: clear the area, then call the inner renderer
/// with an inset popup rect.
fn render_overlay(frame: &mut Frame, area: Rect, inner: impl FnOnce(&mut Frame, Rect)) {
    let w = area.width.saturating_sub(4).min(100);
    let h = area.height.saturating_sub(4);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    frame.render_widget(Clear, popup);
    inner(frame, popup);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let help_text = "\
 Navigation
  Tab/Shift-Tab  Cycle focus area
  j/k or arrows  Move cursor
  ?               Toggle help

 Matrix (main area)
  h/l or arrows   Adjust volume +/-5
  H/L              Adjust volume +/-1
  m                Toggle mute
  d                Set as default input/output

 Overlays
  D     DSP settings
  S     Channel settings
  P     Profiles
  Esc   Close overlay

 Footer sections
  j/k   Navigate items
  1-9   Assign to input
  x     Delete / remove
  u     Unbind

 General
  q     Quit";

    let w = 48.min(area.width.saturating_sub(4));
    let h = 28.min(area.height.saturating_sub(2));
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
