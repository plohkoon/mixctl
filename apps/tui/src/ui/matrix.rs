use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::parse_color;
use crate::app::{AppState, FocusArea};

/// Fixed width for the input label column on the left.
const LABEL_COL_WIDTH: u16 = 14;

/// Minimum width for each output column.
const MIN_COL_WIDTH: u16 = 12;

/// Number of header rows (output name + target + volume bar).
const HEADER_ROWS: u16 = 3;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    if state.inputs.is_empty() && state.outputs.is_empty() {
        let msg = Paragraph::new("  No inputs or outputs configured. Press S to open settings.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(msg, area);
        return;
    }

    let num_outputs = state.outputs.len();
    let num_inputs = state.inputs.len();
    let is_focused = matches!(state.focus, FocusArea::Matrix);

    // Column width for each output
    let remaining = area.width.saturating_sub(LABEL_COL_WIDTH);
    let col_width = if num_outputs > 0 {
        (remaining / num_outputs as u16).max(MIN_COL_WIDTH)
    } else {
        MIN_COL_WIDTH
    };

    // ── Row 0: output headers ──────────────────────────────────────────
    // Top-left corner cell
    let corner = Rect::new(area.x, area.y, LABEL_COL_WIDTH, HEADER_ROWS.min(area.height));
    let corner_label = Paragraph::new("")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(corner_label, corner);

    for (ci, output) in state.outputs.iter().enumerate() {
        let col_idx = ci + 1; // 1-based
        let x = area.x + LABEL_COL_WIDTH + (ci as u16) * col_width;
        if x >= area.x + area.width {
            break;
        }
        let w = col_width.min(area.width.saturating_sub(x - area.x));

        let is_cursor = is_focused && state.matrix_row == 0 && state.matrix_col == col_idx;
        let col_highlight = is_focused && state.matrix_col == col_idx;
        let is_default = output.id == state.default_output;

        let color = parse_color(&output.color);

        // Line 1: output name
        if area.y < area.y + area.height {
            let name_area = Rect::new(x, area.y, w, 1);
            let mut name = truncate(&output.name, w as usize);
            if is_default {
                name = format!("*{}", name);
            }
            let name_style = if is_cursor {
                Style::default().fg(color).bold().reversed()
            } else if col_highlight {
                Style::default().fg(color).bold()
            } else {
                Style::default().fg(color)
            };
            frame.render_widget(Paragraph::new(name).style(name_style), name_area);
        }

        // Line 2: target device (abbreviated)
        if area.height > 1 {
            let target_area = Rect::new(x, area.y + 1, w, 1);
            let target = if output.target_device.is_empty() {
                "(default)".to_string()
            } else {
                abbreviate_device(&output.target_device, w as usize)
            };
            frame.render_widget(
                Paragraph::new(target).style(Style::default().fg(Color::DarkGray)),
                target_area,
            );
        }

        // Line 3: output master volume bar + mute
        if area.height > 2 {
            let bar_area = Rect::new(x, area.y + 2, w, 1);
            render_volume_bar(
                frame,
                bar_area,
                output.volume,
                output.muted,
                color,
                is_cursor,
            );
        }
    }

    // ── Rows 1..=N: input rows ─────────────────────────────────────────
    let body_y = area.y + HEADER_ROWS;
    let body_height = area.height.saturating_sub(HEADER_ROWS);

    for (ri, input) in state.inputs.iter().enumerate() {
        let row_idx = ri + 1; // 1-based
        let y = body_y + ri as u16;
        if y >= area.y + area.height {
            break;
        }

        let is_row_cursor = is_focused && state.matrix_row == row_idx;
        let is_default = input.id == state.default_input;
        let color = parse_color(&input.color);

        // Input label (col 0)
        let label_area = Rect::new(area.x, y, LABEL_COL_WIDTH, 1);
        let is_label_cursor = is_row_cursor && state.matrix_col == 0;

        let mut label = if is_default {
            format!("*{}", input.name)
        } else {
            format!(" {}", input.name)
        };
        label = truncate(&label, LABEL_COL_WIDTH as usize);

        let label_style = if is_label_cursor {
            Style::default().fg(color).bold().reversed()
        } else if is_row_cursor {
            Style::default().fg(color).bold()
        } else {
            Style::default().fg(color)
        };
        frame.render_widget(Paragraph::new(label).style(label_style), label_area);

        // Route cells (col 1..=M)
        for (ci, output) in state.outputs.iter().enumerate() {
            let col_idx = ci + 1; // 1-based
            let x = area.x + LABEL_COL_WIDTH + (ci as u16) * col_width;
            if x >= area.x + area.width {
                break;
            }
            let w = col_width.min(area.width.saturating_sub(x - area.x));
            let cell_area = Rect::new(x, y, w, 1);

            let is_cell_cursor =
                is_focused && state.matrix_row == row_idx && state.matrix_col == col_idx;
            let col_highlight = is_focused && state.matrix_col == col_idx;
            let row_highlight = is_row_cursor;

            // Find route
            let route = state
                .all_routes
                .iter()
                .find(|r| r.input_id == input.id && r.output_id == output.id);

            let (volume, muted) = route
                .map(|r| (r.volume, r.muted))
                .unwrap_or((0, false));

            let bar_color = if muted {
                Color::DarkGray
            } else {
                // Blend between input and output color by using the input color
                parse_color(&input.color)
            };

            if is_cell_cursor {
                render_volume_bar(frame, cell_area, volume, muted, bar_color, true);
            } else if row_highlight || col_highlight {
                // Subtle highlight for cross-hairs
                render_volume_bar_subtle(frame, cell_area, volume, muted, bar_color);
            } else {
                render_volume_bar(frame, cell_area, volume, muted, bar_color, false);
            }
        }
    }

    // Fill remaining body space if inputs don't fill it
    let used_rows = num_inputs as u16;
    if used_rows < body_height {
        let empty_area = Rect::new(area.x, body_y + used_rows, area.width, body_height - used_rows);
        frame.render_widget(
            Paragraph::new("").style(Style::default()),
            empty_area,
        );
    }
}

/// Render a single-line volume bar with mute indicator.
fn render_volume_bar(
    frame: &mut Frame,
    area: Rect,
    volume: u8,
    muted: bool,
    color: Color,
    is_cursor: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Reserve space for volume number display (e.g. " 100" = 4 chars)
    let num_width: u16 = 4;
    let bar_width = area.width.saturating_sub(num_width + 1); // +1 for spacing

    if muted {
        // Show [M] and volume number
        let mute_str = format!("{:>width$} {:>3}", "[M]", volume, width = bar_width as usize);
        let style = if is_cursor {
            Style::default().fg(Color::Red).reversed()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(Paragraph::new(mute_str).style(style), area);
        return;
    }

    let bar_text = build_bar(volume, bar_width as usize);
    let vol_str = format!("{:>3}", volume);

    let bg_style = if is_cursor {
        Style::default().bg(Color::Rgb(40, 40, 50))
    } else {
        Style::default()
    };

    let line = Line::from(vec![
        Span::styled(bar_text, Style::default().fg(color)),
        Span::styled(" ", bg_style),
        Span::styled(
            vol_str,
            Style::default().fg(if is_cursor { Color::White } else { Color::Gray }),
        ),
    ]);

    let para = Paragraph::new(line).style(bg_style);
    frame.render_widget(para, area);
}

/// Volume bar with subtle cross-hair highlighting (dimmer background).
fn render_volume_bar_subtle(
    frame: &mut Frame,
    area: Rect,
    volume: u8,
    muted: bool,
    color: Color,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let num_width: u16 = 4;
    let bar_width = area.width.saturating_sub(num_width + 1);

    let bg = Style::default().bg(Color::Rgb(28, 28, 35));

    if muted {
        let mute_str = format!("{:>width$} {:>3}", "[M]", volume, width = bar_width as usize);
        frame.render_widget(
            Paragraph::new(mute_str).style(bg.fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let bar_text = build_bar(volume, bar_width as usize);
    let vol_str = format!("{:>3}", volume);

    let line = Line::from(vec![
        Span::styled(bar_text, bg.fg(color)),
        Span::styled(" ", bg),
        Span::styled(vol_str, bg.fg(Color::Gray)),
    ]);

    frame.render_widget(Paragraph::new(line).style(bg), area);
}

/// Build a Unicode volume bar string using block characters.
/// Uses full blocks (\u{2588}) and partial blocks for sub-character precision.
fn build_bar(volume: u8, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    // Each character position = 100 / width percent
    // Use eighth-block precision: 8 sub-positions per character
    let total_eighths = (volume as usize * width * 8) / 100;
    let full_blocks = total_eighths / 8;
    let remainder = total_eighths % 8;

    let mut bar = String::with_capacity(width * 4); // UTF-8 can be multi-byte

    // Full block characters
    for _ in 0..full_blocks.min(width) {
        bar.push('\u{2588}'); // Full block
    }

    // Partial block for the fractional part
    if full_blocks < width && remainder > 0 {
        let partial = match remainder {
            1 => '\u{258F}', // Left one eighth block
            2 => '\u{258E}', // Left one quarter block
            3 => '\u{258D}', // Left three eighths block
            4 => '\u{258C}', // Left half block
            5 => '\u{258B}', // Left five eighths block
            6 => '\u{258A}', // Left three quarters block
            7 => '\u{2589}', // Left seven eighths block
            _ => ' ',
        };
        bar.push(partial);
    }

    // Pad remaining with spaces
    let filled = if remainder > 0 {
        full_blocks + 1
    } else {
        full_blocks
    };
    for _ in filled..width {
        bar.push(' ');
    }

    bar
}

/// Truncate a string to fit within `max_len` characters, adding ellipsis if needed.
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else if max_len > 1 {
        let truncated: String = s.chars().take(max_len - 1).collect();
        format!("{}\u{2026}", truncated) // ellipsis
    } else {
        s.chars().take(max_len).collect()
    }
}

/// Abbreviate a PipeWire device name for compact display.
/// e.g. "alsa_output.pci-0000_00_1b.0.analog-stereo" -> "analog-stereo"
fn abbreviate_device(name: &str, max_len: usize) -> String {
    // Try to extract the last meaningful segment
    let short = name
        .rsplit('.')
        .next()
        .unwrap_or(name);
    truncate(short, max_len)
}
