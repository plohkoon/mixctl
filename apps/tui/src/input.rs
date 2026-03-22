use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppAction, AppState, Panel};

pub fn handle_key(key: KeyEvent, state: &AppState) -> Option<AppAction> {
    // Help overlay captures all keys except ? and q
    if state.show_help {
        return match key.code {
            KeyCode::Char('?') | KeyCode::Esc => Some(AppAction::ShowHelp),
            KeyCode::Char('q') => Some(AppAction::Quit),
            _ => None,
        };
    }

    // Panel-specific keys (unique to each panel) — return early if matched,
    // otherwise fall through to the common handler below.
    match state.active_panel {
        Panel::Rules => match key.code {
            KeyCode::Char('d') => return Some(AppAction::DeleteRule),
            KeyCode::Char(c @ '1'..='9') => {
                let n = (c as usize) - ('0' as usize);
                return Some(AppAction::AssignRuleToInput(n));
            }
            _ => {}
        },
        Panel::Capture => match key.code {
            KeyCode::Char('u') => return Some(AppAction::UnbindCapture),
            KeyCode::Char(c @ '1'..='9') => {
                let n = (c as usize) - ('0' as usize);
                return Some(AppAction::BindCapture(n));
            }
            _ => {}
        },
        Panel::Settings => match key.code {
            KeyCode::Char('c') => {
                if state.settings_cursor < state.inputs.len() {
                    return Some(AppAction::CycleInputColor);
                } else {
                    return Some(AppAction::CycleOutputColor);
                }
            }
            _ => {}
        },
        Panel::Dsp => match key.code {
            KeyCode::Char('e') => return Some(AppAction::ToggleEq),
            KeyCode::Char('g') => return Some(AppAction::ToggleGate),
            KeyCode::Char('d') => return Some(AppAction::ToggleDeesser),
            KeyCode::Char('c') => return Some(AppAction::ToggleCompressor),
            KeyCode::Char('l') => return Some(AppAction::ToggleLimiter),
            _ => {}
        },
        _ => {}
    }

    match key.code {
        KeyCode::Char('q') => Some(AppAction::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(AppAction::Quit),
        KeyCode::Tab => Some(AppAction::NextPanel),
        KeyCode::BackTab => Some(AppAction::PrevPanel),
        KeyCode::Char('?') => Some(AppAction::ShowHelp),

        // Output tab selection
        KeyCode::Char(c @ '1'..='9') => {
            let idx = (c as usize) - ('1' as usize);
            Some(AppAction::SelectOutputTab(idx))
        }

        // Navigation
        KeyCode::Char('k') | KeyCode::Up => Some(AppAction::CursorUp),
        KeyCode::Char('j') | KeyCode::Down => Some(AppAction::CursorDown),

        // Volume (context-dependent on panel)
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
            if matches!(state.active_panel, Panel::Routes | Panel::Outputs) {
                Some(AppAction::VolumeUp { fine: false })
            } else {
                None
            }
        }
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('-') => {
            if matches!(state.active_panel, Panel::Routes | Panel::Outputs) {
                Some(AppAction::VolumeDown { fine: false })
            } else {
                None
            }
        }
        KeyCode::Char('L') => {
            if matches!(state.active_panel, Panel::Routes | Panel::Outputs) {
                Some(AppAction::VolumeUp { fine: true })
            } else {
                None
            }
        }
        KeyCode::Char('H') => {
            if matches!(state.active_panel, Panel::Routes | Panel::Outputs) {
                Some(AppAction::VolumeDown { fine: true })
            } else {
                None
            }
        }

        // Mute
        KeyCode::Char('m') => Some(AppAction::ToggleMute),
        KeyCode::Char('M') => Some(AppAction::ToggleOutputMute),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use mixctl_core::{InputInfo, OutputInfo, RouteInfo};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[allow(dead_code)]
    fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn test_state_with_panel(panel: Panel) -> AppState {
        let mut state = AppState::new(
            vec![InputInfo { id: 1, name: "Sys".into(), color: "#000".into() }],
            vec![OutputInfo { id: 5, name: "Out".into(), color: "#fff".into(), volume: 100, muted: false, target_device: String::new() }],
            vec![RouteInfo { input_id: 1, output_id: 5, volume: 80, muted: false }],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        state.active_panel = panel;
        state
    }

    #[test]
    fn q_quits_in_all_panels() {
        let panels = [
            Panel::Routes,
            Panel::Streams,
            Panel::Outputs,
            Panel::Rules,
            Panel::Capture,
            Panel::Settings,
            Panel::Dsp,
        ];
        for panel in panels {
            let state = test_state_with_panel(panel);
            let action = handle_key(key(KeyCode::Char('q')), &state);
            assert!(
                matches!(action, Some(AppAction::Quit)),
                "q should quit in {:?} panel",
                panel
            );
        }
    }

    #[test]
    fn tab_cycles_panel() {
        let state = test_state_with_panel(Panel::Routes);
        let action = handle_key(key(KeyCode::Tab), &state);
        assert!(matches!(action, Some(AppAction::NextPanel)));
    }

    #[test]
    fn backtab_cycles_backward() {
        let state = test_state_with_panel(Panel::Routes);
        let action = handle_key(key(KeyCode::BackTab), &state);
        assert!(matches!(action, Some(AppAction::PrevPanel)));
    }

    #[test]
    fn hjkl_navigates() {
        let state = test_state_with_panel(Panel::Streams);
        assert!(matches!(
            handle_key(key(KeyCode::Char('j')), &state),
            Some(AppAction::CursorDown)
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('k')), &state),
            Some(AppAction::CursorUp)
        ));
    }

    #[test]
    fn arrows_navigate() {
        let state = test_state_with_panel(Panel::Streams);
        assert!(matches!(
            handle_key(key(KeyCode::Down), &state),
            Some(AppAction::CursorDown)
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Up), &state),
            Some(AppAction::CursorUp)
        ));
    }

    #[test]
    fn volume_keys_in_routes() {
        let state = test_state_with_panel(Panel::Routes);
        assert!(matches!(
            handle_key(key(KeyCode::Char('l')), &state),
            Some(AppAction::VolumeUp { fine: false })
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('h')), &state),
            Some(AppAction::VolumeDown { fine: false })
        ));
    }

    #[test]
    fn mute_key_works() {
        let state = test_state_with_panel(Panel::Routes);
        assert!(matches!(
            handle_key(key(KeyCode::Char('m')), &state),
            Some(AppAction::ToggleMute)
        ));
    }

    #[test]
    fn number_keys_select_output() {
        // In a non-panel-specific context (e.g., Streams panel where 1-9 aren't overridden)
        let state = test_state_with_panel(Panel::Streams);
        let action = handle_key(key(KeyCode::Char('1')), &state);
        assert!(matches!(action, Some(AppAction::SelectOutputTab(0))));
        let action = handle_key(key(KeyCode::Char('9')), &state);
        assert!(matches!(action, Some(AppAction::SelectOutputTab(8))));
    }

    #[test]
    fn help_key_toggles() {
        let state = test_state_with_panel(Panel::Routes);
        assert!(matches!(
            handle_key(key(KeyCode::Char('?')), &state),
            Some(AppAction::ShowHelp)
        ));
    }

    #[test]
    fn rules_panel_d_deletes() {
        let state = test_state_with_panel(Panel::Rules);
        assert!(matches!(
            handle_key(key(KeyCode::Char('d')), &state),
            Some(AppAction::DeleteRule)
        ));
    }

    #[test]
    fn capture_panel_u_unbinds() {
        let state = test_state_with_panel(Panel::Capture);
        assert!(matches!(
            handle_key(key(KeyCode::Char('u')), &state),
            Some(AppAction::UnbindCapture)
        ));
    }

    #[test]
    fn dsp_panel_e_toggles_eq() {
        let state = test_state_with_panel(Panel::Dsp);
        assert!(matches!(
            handle_key(key(KeyCode::Char('e')), &state),
            Some(AppAction::ToggleEq)
        ));
    }
}
