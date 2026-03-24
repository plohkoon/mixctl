use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::find_input_display;
use crate::app::{AppState, FocusArea};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(40, 40, 50)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    // Divide the inner area into rows for each section
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Streams
            Constraint::Length(1), // Capture + Playback (side by side)
            Constraint::Length(1), // Rules
            Constraint::Min(0),   // overflow
        ])
        .split(inner);

    // ── Line 1: Streams ────────────────────────────────────────────────
    render_streams_line(frame, chunks[0], state);

    // ── Line 2: Capture + Playback (side by side) ──────────────────────
    if chunks[1].height > 0 {
        let halves = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(chunks[1]);
        render_capture_line(frame, halves[0], state);
        render_playback_line(frame, halves[1], state);
    }

    // ── Line 3: Rules ──────────────────────────────────────────────────
    if chunks[2].height > 0 {
        render_rules_line(frame, chunks[2], state);
    }
}

/// Render the streams section as a single line.
fn render_streams_line(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = matches!(state.focus, FocusArea::Streams);

    // Filter out internal streams
    let visible: Vec<_> = state
        .streams
        .iter()
        .filter(|s| !s.app_name.contains("mixctl.") && !s.app_name.starts_with("output."))
        .collect();

    let header_style = if is_focused {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut spans = vec![
        Span::styled(format!(" Streams({}): ", visible.len()), header_style),
    ];

    if visible.is_empty() {
        spans.push(Span::styled("(none)", Style::default().fg(Color::DarkGray)));
    } else {
        for (i, stream) in visible.iter().enumerate() {
            let (input_name, input_color) = find_input_display(&state.inputs, stream.input_id);
            let is_cursor = is_focused && i == state.footer_cursor;

            if i > 0 {
                spans.push(Span::styled("  ", Style::default()));
            }

            let app_style = if is_cursor {
                Style::default().fg(Color::White).bold().reversed()
            } else {
                Style::default().fg(Color::Gray)
            };
            let arrow_style = Style::default().fg(Color::DarkGray);

            spans.push(Span::styled(
                truncate_inline(&stream.app_name, 12),
                app_style,
            ));
            spans.push(Span::styled("\u{2192}", arrow_style));
            spans.push(Span::styled(input_name, Style::default().fg(input_color)));
        }
    }

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}

/// Render capture devices as a compact line.
fn render_capture_line(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = matches!(state.focus, FocusArea::Capture);

    // Show only added capture devices
    let added: Vec<_> = state
        .capture_devices
        .iter()
        .filter(|c| c.is_added)
        .collect();

    let header_style = if is_focused {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut spans = vec![
        Span::styled(format!(" Capture({}): ", added.len()), header_style),
    ];

    if added.is_empty() {
        spans.push(Span::styled("(none)", Style::default().fg(Color::DarkGray)));
    } else {
        for (i, device) in state.capture_devices.iter().enumerate() {
            if !device.is_added {
                continue;
            }
            let is_cursor = is_focused && i == state.footer_cursor;

            if spans.len() > 1 {
                spans.push(Span::styled("  ", Style::default()));
            }

            let dot_style = Style::default().fg(Color::Green);
            let name_style = if is_cursor {
                Style::default().fg(Color::White).bold().reversed()
            } else {
                Style::default().fg(Color::Gray)
            };

            spans.push(Span::styled("\u{25CF}", dot_style));

            let display = if device.input_id > 0 {
                let (input_name, input_color) = find_input_display(&state.inputs, device.input_id);
                vec![
                    Span::styled(
                        truncate_inline(&device.name, 10),
                        name_style,
                    ),
                    Span::styled("\u{2192}", Style::default().fg(Color::DarkGray)),
                    Span::styled(input_name, Style::default().fg(input_color)),
                ]
            } else {
                vec![Span::styled(
                    truncate_inline(&device.name, 10),
                    name_style,
                )]
            };

            spans.extend(display);
        }
    }

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}

/// Render playback devices as a compact line.
fn render_playback_line(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = matches!(state.focus, FocusArea::Playback);

    let header_style = if is_focused {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut spans = vec![
        Span::styled(
            format!("Playback({}): ", state.playback_devices.len()),
            header_style,
        ),
    ];

    if state.playback_devices.is_empty() {
        spans.push(Span::styled("(none)", Style::default().fg(Color::DarkGray)));
    } else {
        for (i, device) in state.playback_devices.iter().enumerate() {
            let is_cursor = is_focused && i == state.footer_cursor;

            if i > 0 {
                spans.push(Span::styled("  ", Style::default()));
            }

            let name_style = if is_cursor {
                Style::default().fg(Color::White).bold().reversed()
            } else {
                Style::default().fg(Color::Gray)
            };

            // Check if any output targets this device
            let bound_output = state
                .outputs
                .iter()
                .find(|o| o.target_device == device.device_name);

            spans.push(Span::styled(
                truncate_inline(&device.name, 12),
                name_style,
            ));

            if let Some(output) = bound_output {
                let color = super::parse_color(&output.color);
                spans.push(Span::styled("\u{2192}", Style::default().fg(Color::DarkGray)));
                spans.push(Span::styled(
                    truncate_inline(&output.name, 8),
                    Style::default().fg(color),
                ));
            }
        }
    }

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}

/// Render rules as a compact line.
fn render_rules_line(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_focused = matches!(state.focus, FocusArea::Rules);

    let header_style = if is_focused {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut spans = vec![
        Span::styled(format!(" Rules({}): ", state.rules.len()), header_style),
    ];

    if state.rules.is_empty() {
        spans.push(Span::styled("(none)", Style::default().fg(Color::DarkGray)));
    } else {
        for (i, rule) in state.rules.iter().enumerate() {
            let (input_name, input_color) = find_input_display(&state.inputs, rule.input_id);
            let is_cursor = is_focused && i == state.footer_cursor;

            if i > 0 {
                spans.push(Span::styled("  ", Style::default()));
            }

            let app_style = if is_cursor {
                Style::default().fg(Color::White).bold().reversed()
            } else {
                Style::default().fg(Color::Gray)
            };

            spans.push(Span::styled(
                truncate_inline(&rule.app_name, 12),
                app_style,
            ));
            spans.push(Span::styled(
                "\u{2192}",
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(input_name, Style::default().fg(input_color)));
        }
    }

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}

/// Truncate a string inline (no padding), with ellipsis if too long.
fn truncate_inline(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len > 1 {
        let truncated: String = s.chars().take(max_len - 1).collect();
        format!("{}\u{2026}", truncated)
    } else {
        s.chars().take(max_len).collect()
    }
}
