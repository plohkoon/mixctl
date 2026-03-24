use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppAction, AppState, FocusArea, Overlay};

pub fn handle_key(key: KeyEvent, state: &AppState) -> Option<AppAction> {
    // -----------------------------------------------------------------------
    // 1. Profile name text input (typing a new profile name)
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Profiles && state.profile_name_buf.is_some() {
        return match key.code {
            KeyCode::Enter => Some(AppAction::ProfileConfirmName),
            KeyCode::Esc => Some(AppAction::ProfileCancelName),
            KeyCode::Backspace => Some(AppAction::ProfileNameBackspace),
            KeyCode::Char(c) => Some(AppAction::ProfileNameChar(c)),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 2. Settings rename text input
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Settings && state.rename_buf.is_some() {
        return match key.code {
            KeyCode::Enter => Some(AppAction::ConfirmRename),
            KeyCode::Esc => Some(AppAction::CancelRename),
            KeyCode::Backspace => Some(AppAction::RenameBackspace),
            KeyCode::Char(c) => Some(AppAction::RenameChar(c)),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 3. Help overlay — only ? and Esc close it, q quits
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Help {
        return match key.code {
            KeyCode::Char('?') | KeyCode::Esc => Some(AppAction::ToggleHelp),
            KeyCode::Char('q') => Some(AppAction::Quit),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 4. DSP overlay — editing mode
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Dsp && state.dsp_editing {
        return match key.code {
            KeyCode::Esc => Some(AppAction::ExitDspEdit),
            KeyCode::Char('j') | KeyCode::Down => Some(AppAction::DspParamNext),
            KeyCode::Char('k') | KeyCode::Up => Some(AppAction::DspParamPrev),
            KeyCode::Char('l') | KeyCode::Right => Some(AppAction::DspValueUp { fine: false }),
            KeyCode::Char('h') | KeyCode::Left => Some(AppAction::DspValueDown { fine: false }),
            KeyCode::Char('L') => Some(AppAction::DspValueUp { fine: true }),
            KeyCode::Char('H') => Some(AppAction::DspValueDown { fine: true }),
            KeyCode::Char('R') => Some(AppAction::DspResetEq),
            KeyCode::Char('q') => Some(AppAction::Quit),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 5. DSP overlay — browse mode
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Dsp {
        return match key.code {
            KeyCode::Char('e') => Some(AppAction::ToggleEq),
            KeyCode::Char('g') => Some(AppAction::ToggleGate),
            KeyCode::Char('d') => Some(AppAction::ToggleDeesser),
            KeyCode::Char('c') => Some(AppAction::ToggleCompressor),
            KeyCode::Char('l') => Some(AppAction::ToggleLimiter),
            KeyCode::Char('R') => Some(AppAction::DspResetEq),
            KeyCode::Enter => Some(AppAction::EnterDspEdit),
            KeyCode::Char('j') | KeyCode::Down => Some(AppAction::DspCursorDown),
            KeyCode::Char('k') | KeyCode::Up => Some(AppAction::DspCursorUp),
            KeyCode::Esc => Some(AppAction::CloseOverlay),
            KeyCode::Char('q') => Some(AppAction::Quit),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 6. Settings overlay
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Settings {
        return match key.code {
            KeyCode::Char('r') => Some(AppAction::StartRename),
            KeyCode::Char('c') => Some(AppAction::CycleColor),
            KeyCode::Char('t') => Some(AppAction::SetOutputTarget),
            KeyCode::Char('a') => Some(AppAction::AddChannel),
            KeyCode::Char('x') | KeyCode::Delete => Some(AppAction::RemoveChannel),
            KeyCode::Char('J') => Some(AppAction::MoveDown),
            KeyCode::Char('K') => Some(AppAction::MoveUp),
            KeyCode::Char('j') | KeyCode::Down => Some(AppAction::CursorDown),
            KeyCode::Char('k') | KeyCode::Up => Some(AppAction::CursorUp),
            KeyCode::Esc => Some(AppAction::CloseOverlay),
            KeyCode::Char('q') => Some(AppAction::Quit),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 7. Beacn overlay — editing mode
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Beacn && state.beacn_editing {
        return match key.code {
            KeyCode::Char('h') | KeyCode::Left => Some(AppAction::BeacnCycleActionBack),
            KeyCode::Char('l') | KeyCode::Right => Some(AppAction::BeacnCycleAction),
            KeyCode::Char('j') | KeyCode::Down => Some(AppAction::BeacnDown),
            KeyCode::Char('k') | KeyCode::Up => Some(AppAction::BeacnUp),
            KeyCode::Enter => Some(AppAction::BeacnToggleEdit),
            KeyCode::Esc => Some(AppAction::CloseOverlay),
            KeyCode::Char('q') => Some(AppAction::Quit),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 8. Beacn overlay — browse mode
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Beacn {
        return match key.code {
            KeyCode::Char('j') | KeyCode::Down => Some(AppAction::BeacnDown),
            KeyCode::Char('k') | KeyCode::Up => Some(AppAction::BeacnUp),
            KeyCode::Char('h') | KeyCode::Left => Some(AppAction::BeacnLeft),
            KeyCode::Char('l') | KeyCode::Right => Some(AppAction::BeacnRight),
            KeyCode::Enter => Some(AppAction::BeacnToggleEdit),
            KeyCode::Esc => Some(AppAction::CloseOverlay),
            KeyCode::Char('q') => Some(AppAction::Quit),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 9. Profiles overlay
    // -----------------------------------------------------------------------
    if state.overlay == Overlay::Profiles {
        return match key.code {
            KeyCode::Char('s') => Some(AppAction::ProfileSave),
            KeyCode::Enter => Some(AppAction::ProfileLoad),
            KeyCode::Char('x') | KeyCode::Delete => Some(AppAction::ProfileDelete),
            KeyCode::Char('j') | KeyCode::Down => Some(AppAction::CursorDown),
            KeyCode::Char('k') | KeyCode::Up => Some(AppAction::CursorUp),
            KeyCode::Esc => Some(AppAction::CloseOverlay),
            KeyCode::Char('q') => Some(AppAction::Quit),
            _ => None,
        };
    }

    // -----------------------------------------------------------------------
    // 8. No overlay — normal mode
    // -----------------------------------------------------------------------

    // Global keys first
    match key.code {
        KeyCode::Char('q') => return Some(AppAction::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Some(AppAction::Quit);
        }
        KeyCode::Char('?') => return Some(AppAction::ToggleHelp),
        KeyCode::Tab => return Some(AppAction::NextFocus),
        KeyCode::BackTab => return Some(AppAction::PrevFocus),
        KeyCode::Char('D') => return Some(AppAction::OpenDsp),
        KeyCode::Char('S') => return Some(AppAction::OpenSettings),
        KeyCode::Char('P') => return Some(AppAction::OpenProfiles),
        KeyCode::Char('B') if state.beacn_connected => return Some(AppAction::OpenBeacn),
        _ => {}
    }

    match state.focus {
        FocusArea::Matrix => match key.code {
            // Arrow keys navigate the matrix grid
            KeyCode::Up => Some(AppAction::CursorUp),
            KeyCode::Down => Some(AppAction::CursorDown),
            KeyCode::Left => Some(AppAction::CursorLeft),
            KeyCode::Right => Some(AppAction::CursorRight),
            // j/k for row navigation (vim style)
            KeyCode::Char('j') => Some(AppAction::CursorDown),
            KeyCode::Char('k') => Some(AppAction::CursorUp),
            // h/l for volume adjustment
            KeyCode::Char('h') => Some(AppAction::VolumeDown { fine: false }),
            KeyCode::Char('l') => Some(AppAction::VolumeUp { fine: false }),
            KeyCode::Char('H') => Some(AppAction::VolumeDown { fine: true }),
            KeyCode::Char('L') => Some(AppAction::VolumeUp { fine: true }),
            // Mute
            KeyCode::Char('m') => Some(AppAction::ToggleMute),
            // Channel management
            KeyCode::Char('a') => Some(AppAction::AddChannel),
            KeyCode::Char('d') => Some(AppAction::SetDefault),
            _ => None,
        },
        // Footer sections: Streams, Capture, Playback, Rules
        FocusArea::Streams | FocusArea::Capture | FocusArea::Playback | FocusArea::Rules => {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => Some(AppAction::FooterDown),
                KeyCode::Char('k') | KeyCode::Up => Some(AppAction::FooterUp),
                KeyCode::Char(c @ '1'..='9') => {
                    let n = (c as usize) - ('0' as usize);
                    Some(AppAction::AssignToInput(n))
                }
                KeyCode::Char('x') | KeyCode::Delete => Some(AppAction::DeleteItem),
                KeyCode::Char('u') => Some(AppAction::UnbindItem),
                _ => None,
            }
        }
    }
}
