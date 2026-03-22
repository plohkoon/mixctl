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

    // Panel-specific keys
    match state.active_panel {
        Panel::Rules => {
            match key.code {
                KeyCode::Char('q') => return Some(AppAction::Quit),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(AppAction::Quit),
                KeyCode::Tab => return Some(AppAction::NextPanel),
                KeyCode::BackTab => return Some(AppAction::PrevPanel),
                KeyCode::Char('?') => return Some(AppAction::ShowHelp),
                KeyCode::Char('k') | KeyCode::Up => return Some(AppAction::CursorUp),
                KeyCode::Char('j') | KeyCode::Down => return Some(AppAction::CursorDown),
                KeyCode::Char('d') => return Some(AppAction::DeleteRule),
                KeyCode::Char(c @ '1'..='9') => {
                    let n = (c as usize) - ('0' as usize);
                    return Some(AppAction::AssignRuleToInput(n));
                }
                _ => return None,
            }
        }
        Panel::Capture => {
            match key.code {
                KeyCode::Char('q') => return Some(AppAction::Quit),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(AppAction::Quit),
                KeyCode::Tab => return Some(AppAction::NextPanel),
                KeyCode::BackTab => return Some(AppAction::PrevPanel),
                KeyCode::Char('?') => return Some(AppAction::ShowHelp),
                KeyCode::Char('k') | KeyCode::Up => return Some(AppAction::CursorUp),
                KeyCode::Char('j') | KeyCode::Down => return Some(AppAction::CursorDown),
                KeyCode::Char('u') => return Some(AppAction::UnbindCapture),
                KeyCode::Char(c @ '1'..='9') => {
                    let n = (c as usize) - ('0' as usize);
                    return Some(AppAction::BindCapture(n));
                }
                _ => return None,
            }
        }
        Panel::Settings => {
            match key.code {
                KeyCode::Char('q') => return Some(AppAction::Quit),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(AppAction::Quit),
                KeyCode::Tab => return Some(AppAction::NextPanel),
                KeyCode::BackTab => return Some(AppAction::PrevPanel),
                KeyCode::Char('?') => return Some(AppAction::ShowHelp),
                KeyCode::Char('k') | KeyCode::Up => return Some(AppAction::CursorUp),
                KeyCode::Char('j') | KeyCode::Down => return Some(AppAction::CursorDown),
                KeyCode::Char('c') => {
                    if state.settings_cursor < state.inputs.len() {
                        return Some(AppAction::CycleInputColor);
                    } else {
                        return Some(AppAction::CycleOutputColor);
                    }
                }
                _ => return None,
            }
        }
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
