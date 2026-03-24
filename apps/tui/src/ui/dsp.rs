use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine, Points};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use mixctl_core::{compute_eq_curve, EqBandInfo};

use crate::app::{AppState, Panel};

fn enabled_tag(enabled: bool) -> Span<'static> {
    if enabled {
        Span::styled("[ON]", Style::default().fg(Color::Green))
    } else {
        Span::styled("[OFF]", Style::default().fg(Color::DarkGray))
    }
}

/// Render the EQ frequency response canvas for the given bands.
fn render_eq_curve(frame: &mut Frame, area: Rect, bands: &[EqBandInfo]) {
    // Reserve left column for dB labels and bottom row for frequency labels.
    let label_w: u16 = 5; // e.g. "+24 "
    let label_h: u16 = 1; // frequency labels row

    // If the area is too small, skip rendering entirely.
    if area.width <= label_w + 2 || area.height <= label_h + 2 {
        return;
    }

    let canvas_area = Rect {
        x: area.x + label_w,
        y: area.y,
        width: area.width - label_w,
        height: area.height - label_h,
    };

    let freq_label_area = Rect {
        x: area.x + label_w,
        y: area.y + area.height - label_h,
        width: area.width - label_w,
        height: label_h,
    };

    let x_min = 20.0_f64.log10();
    let x_max = 20000.0_f64.log10();

    // Compute the curve once; the closure captures the result by reference.
    let curve = compute_eq_curve(bands);
    let band_markers: Vec<(f64, f64)> = bands
        .iter()
        .map(|b| (b.frequency.log10(), b.gain_db))
        .collect();

    let canvas = Canvas::default()
        .block(
            Block::default()
                .title(" EQ Response ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .x_bounds([x_min, x_max])
        .y_bounds([-24.0, 24.0])
        .marker(Marker::Braille)
        .paint(|ctx| {
            // Grid lines at +/-6, 12, 18 dB
            let grid_color = Color::Rgb(30, 30, 35);
            for db in [-18.0, -12.0, -6.0, 6.0, 12.0, 18.0] {
                ctx.draw(&CanvasLine {
                    x1: x_min,
                    y1: db,
                    x2: x_max,
                    y2: db,
                    color: grid_color,
                });
            }
            // Frequency grid lines at 100, 1k, 10k Hz
            for freq in [100.0_f64, 1000.0, 10000.0] {
                let x = freq.log10();
                ctx.draw(&CanvasLine {
                    x1: x,
                    y1: -24.0,
                    x2: x,
                    y2: 24.0,
                    color: grid_color,
                });
            }
            // 0dB reference line
            ctx.draw(&CanvasLine {
                x1: x_min,
                y1: 0.0,
                x2: x_max,
                y2: 0.0,
                color: Color::DarkGray,
            });

            // EQ response curve as connected line segments
            for i in 1..curve.len() {
                let (f0, db0) = curve[i - 1];
                let (f1, db1) = curve[i];
                ctx.draw(&CanvasLine {
                    x1: f0.log10(),
                    y1: db0,
                    x2: f1.log10(),
                    y2: db1,
                    color: Color::Cyan,
                });
            }

            // Band marker points
            if !band_markers.is_empty() {
                ctx.draw(&Points {
                    coords: &band_markers,
                    color: Color::Yellow,
                });
            }
        });

    frame.render_widget(canvas, canvas_area);

    // -- dB labels on the left side --
    let db_labels = [
        ("+24", -24.0_f64),
        ("+12", -12.0),
        ("  0", 0.0),
        ("-12", 12.0),
        ("-24", 24.0),
    ];
    // The canvas inner area (inside its border) determines mapping.
    let inner_top = canvas_area.y + 1; // border
    let inner_h = canvas_area.height.saturating_sub(2) as f64; // border top+bottom
    for (label, db_norm) in &db_labels {
        // Map db_norm from y_bounds [-24..24] to pixel row.
        // y_bounds bottom=-24 maps to inner_top + inner_h - 1
        // y_bounds top=+24 maps to inner_top
        let frac = (db_norm - (-24.0)) / 48.0; // 0.0 at bottom, 1.0 at top
        let row = inner_top as f64 + frac * (inner_h - 1.0);
        let row = row.round() as u16;
        if row >= area.y && row < area.y + area.height {
            let p = Paragraph::new(*label).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(
                p,
                Rect {
                    x: area.x,
                    y: row,
                    width: label_w,
                    height: 1,
                },
            );
        }
    }

    // -- Frequency labels along the bottom --
    let freq_labels: &[(&str, f64)] = &[
        ("20", 20.0),
        ("100", 100.0),
        ("1k", 1000.0),
        ("10k", 10000.0),
        ("20k", 20000.0),
    ];
    let inner_left = canvas_area.x + 1; // border
    let inner_w = canvas_area.width.saturating_sub(2) as f64;
    // Render each frequency label as a small Paragraph at the computed x position.
    for (label, freq) in freq_labels {
        let frac = (freq.log10() - x_min) / (x_max - x_min);
        let col = inner_left as f64 + frac * (inner_w - 1.0);
        let col = col.round() as u16;
        let lbl_len = label.len() as u16;
        // Center the label on the column.
        let lbl_x = col.saturating_sub(lbl_len / 2);
        if lbl_x >= freq_label_area.x
            && lbl_x + lbl_len <= freq_label_area.x + freq_label_area.width
        {
            let p = Paragraph::new(*label).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(
                p,
                Rect {
                    x: lbl_x,
                    y: freq_label_area.y,
                    width: lbl_len,
                    height: 1,
                },
            );
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_active = matches!(state.active_panel, Panel::Dsp);
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" DSP ")
        .borders(Borders::ALL)
        .border_style(border_style);

    if state.inputs.is_empty() && state.outputs.is_empty() {
        let text = Paragraph::new("  (no inputs or outputs)")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(text, area);
        return;
    }

    // Determine if the cursor is on an input (for the EQ curve).
    let selected_input = if state.dsp_cursor < state.inputs.len() {
        state.inputs.get(state.dsp_cursor)
    } else {
        None
    };

    let eq_bands: Vec<EqBandInfo> = selected_input
        .and_then(|inp| state.dsp_input_eq.get(&inp.id))
        .map(|(_, bands)| bands.clone())
        .unwrap_or_default();

    // Split area: EQ curve (top 45%) + parameter list (bottom 55%).
    // Only show the curve when there is enough vertical space and an input is selected.
    let show_curve = selected_input.is_some() && area.height >= 12;

    let (curve_area, list_area) = if show_curve {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    };

    // Render EQ curve if applicable.
    if let Some(ca) = curve_area {
        render_eq_curve(frame, ca, &eq_bands);
    }

    // -- Build the parameter list (existing content) --
    let mut items: Vec<ListItem> = Vec::new();

    // -- Input DSP sections --
    for (i, input) in state.inputs.iter().enumerate() {
        let is_selected = is_active && state.dsp_cursor == i;
        let cursor = if is_selected { "\u{25b8} " } else { "  " };
        let name_style =
            Style::default().fg(if is_selected { Color::White } else { Color::Gray });

        // Header line for this input
        items.push(ListItem::new(Line::from(vec![
            Span::raw(cursor),
            Span::styled(
                format!("Input: {}", input.name),
                name_style.add_modifier(Modifier::BOLD),
            ),
        ])));

        let (eq_enabled, eq_bands) = state
            .dsp_input_eq
            .get(&input.id)
            .cloned()
            .unwrap_or((false, Vec::new()));
        let gate = state.dsp_input_gate.get(&input.id);
        let deesser = state.dsp_input_deesser.get(&input.id);

        // Toggle status line
        let gate_enabled = gate.map(|g| g.enabled).unwrap_or(false);
        let deesser_enabled = deesser.map(|d| d.enabled).unwrap_or(false);

        items.push(ListItem::new(Line::from(vec![
            Span::raw("    EQ: "),
            enabled_tag(eq_enabled),
            Span::raw("  Gate: "),
            enabled_tag(gate_enabled),
            Span::raw("  De-esser: "),
            enabled_tag(deesser_enabled),
        ])));

        // EQ bands (show up to 8)
        for (band_idx, band) in eq_bands.iter().enumerate() {
            let gain_str = if band.gain_db >= 0.0 {
                format!("+{:.1}dB", band.gain_db)
            } else {
                format!("{:.1}dB", band.gain_db)
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("    Band {}: ", band_idx + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<12}", band.band_type),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(
                    format!("{:>7.0}Hz ", band.frequency),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(
                    format!("{:>8} ", gain_str),
                    Style::default().fg(if band.gain_db.abs() > 0.01 {
                        Color::Yellow
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(
                    format!("Q={:.2}", band.q),
                    Style::default().fg(Color::DarkGray),
                ),
            ])));
        }

        // Gate parameters
        if let Some(g) = gate {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("    Gate: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "threshold={:.0}dB  attack={:.0}ms  release={:.0}ms  hold={:.0}ms",
                        g.threshold_db, g.attack_ms, g.release_ms, g.hold_ms,
                    ),
                    Style::default().fg(Color::Gray),
                ),
            ])));
        }

        // De-esser parameters
        if let Some(d) = deesser {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("    De-esser: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "freq={:.0}Hz  threshold={:.0}dB  ratio={:.1}:1",
                        d.frequency, d.threshold_db, d.ratio,
                    ),
                    Style::default().fg(Color::Gray),
                ),
            ])));
        }

        // Blank separator
        items.push(ListItem::new(Line::from("")));
    }

    // -- Output DSP sections --
    for (i, output) in state.outputs.iter().enumerate() {
        let cursor_idx = state.inputs.len() + i;
        let is_selected = is_active && state.dsp_cursor == cursor_idx;
        let cursor = if is_selected { "\u{25b8} " } else { "  " };
        let name_style =
            Style::default().fg(if is_selected { Color::White } else { Color::Gray });

        items.push(ListItem::new(Line::from(vec![
            Span::raw(cursor),
            Span::styled(
                format!("Output: {}", output.name),
                name_style.add_modifier(Modifier::BOLD),
            ),
        ])));

        let compressor = state.dsp_output_compressor.get(&output.id);
        let limiter = state.dsp_output_limiter.get(&output.id);

        let comp_enabled = compressor.map(|c| c.enabled).unwrap_or(false);
        let lim_enabled = limiter.map(|l| l.enabled).unwrap_or(false);

        items.push(ListItem::new(Line::from(vec![
            Span::raw("    Compressor: "),
            enabled_tag(comp_enabled),
            Span::raw("  Limiter: "),
            enabled_tag(lim_enabled),
        ])));

        if let Some(c) = compressor {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("    Comp: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "threshold={:.0}dB  ratio={:.1}:1  attack={:.0}ms  release={:.0}ms",
                        c.threshold_db, c.ratio, c.attack_ms, c.release_ms,
                    ),
                    Style::default().fg(Color::Gray),
                ),
            ])));
            items.push(ListItem::new(Line::from(vec![
                Span::styled("          ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "makeup={:.1}dB  knee={:.1}dB",
                        c.makeup_gain_db, c.knee_db,
                    ),
                    Style::default().fg(Color::Gray),
                ),
            ])));
        }

        if let Some(l) = limiter {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("    Limiter: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "ceiling={:.1}dB  release={:.0}ms",
                        l.ceiling_db, l.release_ms,
                    ),
                    Style::default().fg(Color::Gray),
                ),
            ])));
        }

        items.push(ListItem::new(Line::from("")));
    }

    // Show current editing parameter when in DSP edit mode
    if state.dsp_editing {
        if let Some((label, value)) = state.dsp_param_label() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  \u{25b6} Editing: {} = {:.2}", label, value),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
            ])));
        }
    }

    let hint = if state.dsp_editing {
        Line::from(vec![
            Span::styled("j/k", Style::default().fg(Color::Yellow)),
            Span::raw(": param  "),
            Span::styled("h/l", Style::default().fg(Color::Yellow)),
            Span::raw(": adjust  "),
            Span::styled("H/L", Style::default().fg(Color::Yellow)),
            Span::raw(": fine  "),
            Span::styled("R", Style::default().fg(Color::Yellow)),
            Span::raw(": reset EQ  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": done"),
        ])
    } else {
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": edit  "),
            Span::styled("e", Style::default().fg(Color::Yellow)),
            Span::raw(": EQ  "),
            Span::styled("g", Style::default().fg(Color::Yellow)),
            Span::raw(": gate  "),
            Span::styled("d", Style::default().fg(Color::Yellow)),
            Span::raw(": de-esser  "),
            Span::styled("c", Style::default().fg(Color::Yellow)),
            Span::raw(": compressor  "),
            Span::styled("l", Style::default().fg(Color::Yellow)),
            Span::raw(": limiter  "),
            Span::styled("R", Style::default().fg(Color::Yellow)),
            Span::raw(": reset EQ"),
        ])
    };

    let list = List::new(items).block(
        block.title_bottom(hint.alignment(Alignment::Center)),
    );
    frame.render_widget(list, list_area);
}
