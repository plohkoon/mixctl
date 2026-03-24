use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{AppState, FocusArea, Overlay};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let spans = match state.overlay {
        Overlay::Dsp => {
            if state.dsp_editing {
                hint_spans(&[
                    ("j/k", "param"),
                    ("h/l", "adjust"),
                    ("H/L", "fine"),
                    ("R", "reset EQ"),
                    ("Esc", "done"),
                ])
            } else {
                hint_spans(&[
                    ("j/k", "select"),
                    ("e", "eq"),
                    ("g", "gate"),
                    ("d", "de-esser"),
                    ("c", "comp"),
                    ("l", "lim"),
                    ("Enter", "edit"),
                    ("Esc", "close"),
                ])
            }
        }
        Overlay::Settings => {
            if state.rename_buf.is_some() {
                hint_spans(&[("Enter", "confirm"), ("Esc", "cancel")])
            } else {
                hint_spans(&[
                    ("j/k", "nav"),
                    ("r", "rename"),
                    ("c", "color"),
                    ("t", "target"),
                    ("a", "add"),
                    ("x", "del"),
                    ("J/K", "move"),
                    ("Esc", "close"),
                ])
            }
        }
        Overlay::Profiles => {
            if state.profile_name_buf.is_some() {
                hint_spans(&[("Enter", "confirm"), ("Esc", "cancel")])
            } else {
                hint_spans(&[
                    ("j/k", "nav"),
                    ("s", "save"),
                    ("Enter", "load"),
                    ("x", "delete"),
                    ("Esc", "close"),
                ])
            }
        }
        Overlay::Beacn => {
            if state.beacn_editing {
                hint_spans(&[
                    ("h/l", "cycle"),
                    ("j/k", "nav"),
                    ("Enter", "done"),
                    ("Esc", "close"),
                ])
            } else {
                hint_spans(&[
                    ("j/k", "nav"),
                    ("h/l", "field"),
                    ("Enter", "edit"),
                    ("Esc", "close"),
                ])
            }
        }
        Overlay::Help => hint_spans(&[("?/Esc", "close"), ("q", "quit")]),
        Overlay::None => focus_hints(state.focus),
    };

    let line = Line::from(spans);
    let bar = Paragraph::new(line)
        .style(Style::default().bg(Color::Rgb(25, 25, 30)).fg(Color::DarkGray));
    frame.render_widget(bar, area);
}

fn focus_hints(focus: FocusArea) -> Vec<Span<'static>> {
    match focus {
        FocusArea::Matrix => hint_spans(&[
            ("\u{2190}\u{2192}\u{2191}\u{2193}", "nav"),
            ("h/l", "vol"),
            ("H/L", "fine"),
            ("m", "mute"),
            ("D", "dsp"),
            ("S", "settings"),
            ("P", "profiles"),
            ("d", "default"),
            ("?", "help"),
            ("q", "quit"),
        ]),
        FocusArea::Streams => hint_spans(&[
            ("\u{2191}\u{2193}", "nav"),
            ("1-9", "assign"),
            ("Tab", "section"),
            ("q", "quit"),
        ]),
        FocusArea::Capture => hint_spans(&[
            ("\u{2191}\u{2193}", "nav"),
            ("1-9", "bind"),
            ("x", "remove"),
            ("u", "unbind"),
            ("Tab", "section"),
            ("q", "quit"),
        ]),
        FocusArea::Playback => hint_spans(&[
            ("\u{2191}\u{2193}", "nav"),
            ("Tab", "section"),
            ("q", "quit"),
        ]),
        FocusArea::Rules => hint_spans(&[
            ("\u{2191}\u{2193}", "nav"),
            ("1-9", "assign"),
            ("x", "delete"),
            ("Tab", "section"),
            ("q", "quit"),
        ]),
    }
}

/// Build a list of styled spans from key-description pairs.
///
/// Keys are rendered in yellow, descriptions in dark gray, separated by
/// a colon and double-space delimiters between pairs.
fn hint_spans(pairs: &[(&'static str, &'static str)]) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(pairs.len() * 3);
    spans.push(Span::raw(" "));
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(*key, Style::default().fg(Color::Yellow)));
        spans.push(Span::styled(format!(":{desc}"), Style::default().fg(Color::DarkGray)));
    }
    spans
}
