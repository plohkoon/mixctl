use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use mixctl_beacn_display::{DisplayLayout, DisplayState, Patch};
use mixctl_core::config_sections::{ButtonAction, ButtonMapping, ButtonMappings};
use mixctl_protocol::enums::ButtonLighting;
use mixctl_protocol::{parse_input, Button, Color, Command, ImageChunker};
use tracing::{error, info, warn};

use crate::types::{DeviceCommand, DeviceEvent};
use crate::usb::Device;

const IMG_TIMEOUT: Duration = Duration::from_millis(100);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct DeviceThread {
    thread_handle: thread::JoinHandle<()>,
}

impl DeviceThread {
    /// Spawn the device thread with a layout, channels, and shutdown flag.
    pub fn spawn(
        shutdown_flag: Arc<AtomicBool>,
        event_tx: tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
        cmd_rx: tokio::sync::mpsc::UnboundedReceiver<DeviceCommand>,
        layout: Box<dyn DisplayLayout>,
    ) -> Self {
        let thread_handle = thread::spawn(move || {
            run_device_thread(shutdown_flag, event_tx, cmd_rx, layout);
        });
        DeviceThread { thread_handle }
    }

    pub fn join(self) {
        if let Err(e) = self.thread_handle.join() {
            error!("device thread panicked: {:?}", e);
        }
    }
}

fn run_device_thread(
    shutdown_flag: Arc<AtomicBool>,
    event_tx: tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<DeviceCommand>,
    mut layout: Box<dyn DisplayLayout>,
) {
    let mut backoff = Duration::from_secs(2);

    loop {
        if shutdown_flag.load(Ordering::Acquire) {
            break;
        }

        // Try to open device
        let dev = match Device::open() {
            Ok(dev) => dev,
            Err(_) => {
                // Device not found, wait and retry
                thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(30));
                // Drain any commands while waiting
                while cmd_rx.try_recv().is_ok() {}
                continue;
            }
        };

        backoff = Duration::from_secs(2);
        info!("device connected: {}", dev.device_type);

        // Init device
        if let Err(e) = init_device(&dev) {
            warn!("device init failed: {e}");
            continue;
        }

        event_tx.send(DeviceEvent::Connected).ok();

        // Run the inner loop
        let disconnect_reason = run_inner_loop(
            &dev,
            &shutdown_flag,
            &event_tx,
            &mut cmd_rx,
            &mut layout,
        );

        // Shutdown device
        shutdown_device(&dev);

        match disconnect_reason {
            DisconnectReason::Shutdown => {
                info!("device thread: shutdown requested");
                break;
            }
            DisconnectReason::UsbError(e) => {
                warn!("device USB error: {e}");
                event_tx.send(DeviceEvent::Disconnected).ok();
            }
            DisconnectReason::CommandShutdown => {
                info!("device thread: shutdown command received");
                break;
            }
        }
    }

    info!("device thread exiting");
}

enum DisconnectReason {
    Shutdown,
    UsbError(String),
    CommandShutdown,
}

fn run_inner_loop(
    dev: &Device,
    shutdown_flag: &Arc<AtomicBool>,
    event_tx: &tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
    cmd_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DeviceCommand>,
    layout: &mut Box<dyn DisplayLayout>,
) -> DisconnectReason {
    let mut current_state: Option<DisplayState> = None;
    let mut prev_state: Option<DisplayState> = None;
    let mut needs_full_redraw = true;
    let mut button_mappings = ButtonMappings::default();
    let mut hold_threshold = Duration::from_millis(200);
    let mut hold_state = ButtonHoldState::new();

    loop {
        if shutdown_flag.load(Ordering::Acquire) {
            return DisconnectReason::Shutdown;
        }

        // Process commands
        loop {
            match cmd_rx.try_recv() {
                Ok(DeviceCommand::UpdateState(new_state)) => {
                    if let Some(ref prev) = current_state {
                        // Check if we need a full redraw (output/page switch)
                        if prev.current_output_index != new_state.current_output_index
                            || prev.page != new_state.page
                        {
                            needs_full_redraw = true;
                        }
                    } else {
                        needs_full_redraw = true;
                    }
                    current_state = Some(new_state);
                }
                Ok(DeviceCommand::ChangeLayout(new_layout)) => {
                    *layout = new_layout;
                    needs_full_redraw = true;
                }
                Ok(DeviceCommand::SetButtonConfig { mappings, hold_threshold: threshold }) => {
                    button_mappings = mappings;
                    hold_threshold = threshold;
                }
                Ok(DeviceCommand::SetBrightness { display, led }) => {
                    let _ = dev.write_command(&Command::DisplayBrightness(display).to_bytes());
                    let _ = dev.write_command(&Command::ButtonLedBrightness(led).to_bytes());
                }
                Ok(DeviceCommand::ShowWaiting) => {
                    let jpeg = mixctl_beacn_display::render::render_waiting_screen();
                    if let Err(e) = send_full(dev, &jpeg) {
                        return DisconnectReason::UsbError(e.to_string());
                    }
                    // Dim LEDs
                    let _ = dev.write_command(&mixctl_protocol::Command::ButtonLedBrightness(30).to_bytes());
                    current_state = None;
                    needs_full_redraw = false;
                }
                Ok(DeviceCommand::Shutdown) => {
                    return DisconnectReason::CommandShutdown;
                }
                Err(_) => break,
            }
        }

        // Render updates if state changed
        if let Some(ref state) = current_state {
            if needs_full_redraw {
                let jpeg = (**layout).render_full(state);
                if let Err(e) = send_full(dev, &jpeg) {
                    return DisconnectReason::UsbError(e.to_string());
                }
                if let Err(e) = update_leds(dev, state) {
                    return DisconnectReason::UsbError(e.to_string());
                }
                needs_full_redraw = false;
                prev_state = Some(state.clone());
            } else if let Some(ref prev) = prev_state {
                if prev != state {
                    let patches = (**layout).render_diff(prev, state);
                    for p in &patches {
                        if let Err(e) = send_patch(dev, p) {
                            return DisconnectReason::UsbError(e.to_string());
                        }
                    }
                    if let Err(e) = update_leds(dev, state) {
                        return DisconnectReason::UsbError(e.to_string());
                    }
                    prev_state = Some(state.clone());
                }
            }
        }

        // Poll for input
        if let Err(e) = dev.write_command(&Command::Poll.to_bytes()) {
            return DisconnectReason::UsbError(e.to_string());
        }

        match dev.read(POLL_INTERVAL) {
            Ok(resp) if resp.len() >= 10 => {
                if let Ok(event) = parse_input(&resp) {
                    // Hold detection state machine: process each button
                    let now = Instant::now();
                    for (idx, &button) in Button::ALL.iter().enumerate() {
                        let is_pressed = button.is_pressed(event.button_mask);
                        let phase = &mut hold_state.phases[idx];
                        let mapping = get_button_mapping(&button_mappings, button);

                        match (*phase, is_pressed) {
                            (ButtonPhase::Idle, true) => {
                                if mapping.hold == ButtonAction::None {
                                    // No hold action: fire press immediately (zero latency)
                                    fire_press_action(event_tx, &mapping.press, button, &current_state);
                                    *phase = ButtonPhase::Held { captured_input_id: None };
                                } else {
                                    *phase = ButtonPhase::Pending { press_time: now };
                                }
                            }
                            (ButtonPhase::Pending { press_time }, true) => {
                                if now.duration_since(press_time) >= hold_threshold {
                                    let captured = capture_input_id(button, &current_state);
                                    fire_hold_action(event_tx, &mapping.hold, button, &current_state);
                                    *phase = ButtonPhase::Held { captured_input_id: captured };
                                }
                            }
                            (ButtonPhase::Pending { .. }, false) => {
                                // Released before threshold: fire press action
                                fire_press_action(event_tx, &mapping.press, button, &current_state);
                                *phase = ButtonPhase::Idle;
                            }
                            (ButtonPhase::Held { captured_input_id }, false) => {
                                // Released after hold: fire release action (PTM/PTT only)
                                fire_release_action(event_tx, &mapping.hold, captured_input_id);
                                *phase = ButtonPhase::Idle;
                            }
                            _ => {} // Idle+not pressed, Held+still pressed: no-op
                        }
                    }

                    // Handle dial events
                    handle_dials(event_tx, &event.dials, &current_state);
                }
            }
            Ok(_) => {}
            Err(e) => {
                if e.downcast_ref::<rusb::Error>() == Some(&rusb::Error::Timeout) {
                    // Normal timeout, no data
                } else {
                    return DisconnectReason::UsbError(e.to_string());
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Hold detection types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum ButtonPhase {
    Idle,
    Pending { press_time: Instant },
    Held { captured_input_id: Option<u32> },
}

struct ButtonHoldState {
    phases: [ButtonPhase; 11],
}

impl ButtonHoldState {
    fn new() -> Self {
        Self { phases: [ButtonPhase::Idle; 11] }
    }
}

// ---------------------------------------------------------------------------
// Button mapping helpers
// ---------------------------------------------------------------------------

fn get_button_mapping(mappings: &ButtonMappings, button: Button) -> &ButtonMapping {
    match button {
        Button::Dial1 => &mappings.dial1,
        Button::Dial2 => &mappings.dial2,
        Button::Dial3 => &mappings.dial3,
        Button::Dial4 => &mappings.dial4,
        Button::Audience1 => &mappings.audience1,
        Button::Audience2 => &mappings.audience2,
        Button::Audience3 => &mappings.audience3,
        Button::Audience4 => &mappings.audience4,
        Button::AudienceMix => &mappings.mix,
        Button::PageLeft => &mappings.page_left,
        Button::PageRight => &mappings.page_right,
    }
}

fn button_slot(button: Button) -> usize {
    match button {
        Button::Dial1 | Button::Audience1 => 0,
        Button::Dial2 | Button::Audience2 => 1,
        Button::Dial3 | Button::Audience3 => 2,
        Button::Dial4 | Button::Audience4 => 3,
        _ => 0,
    }
}

fn capture_input_id(button: Button, state: &Option<DisplayState>) -> Option<u32> {
    let state = state.as_ref()?;
    let slot = button_slot(button);
    state.visible_inputs[slot].as_ref().map(|s| s.input_id)
}

// ---------------------------------------------------------------------------
// Action → DeviceEvent resolution
// ---------------------------------------------------------------------------

fn resolve_action_event(
    action: &ButtonAction,
    slot: usize,
    current_output_id: u32,
    state: &DisplayState,
) -> Option<DeviceEvent> {
    match action {
        ButtonAction::ToggleRouteMute => {
            state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::ToggleRouteMute {
                input_id: s.input_id,
                output_id: current_output_id,
            })
        }
        ButtonAction::ToggleGlobalMute => {
            state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::ToggleGlobalMute {
                input_id: s.input_id,
            })
        }
        ButtonAction::MuteOutput { output_id } => {
            Some(DeviceEvent::ToggleOutputMute { output_id: *output_id })
        }
        ButtonAction::MuteAllOutputs => Some(DeviceEvent::ToggleAllOutputsMute),
        ButtonAction::ToggleEq => {
            state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::ToggleEq { input_id: s.input_id })
        }
        ButtonAction::ToggleGate => {
            state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::ToggleGate { input_id: s.input_id })
        }
        ButtonAction::ToggleDeesser => {
            state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::ToggleDeesser { input_id: s.input_id })
        }
        ButtonAction::ToggleCompressor => {
            Some(DeviceEvent::ToggleCompressor { output_id: current_output_id })
        }
        ButtonAction::ToggleLimiter => {
            Some(DeviceEvent::ToggleLimiter { output_id: current_output_id })
        }
        ButtonAction::LoadProfile { name } => {
            Some(DeviceEvent::LoadProfile { name: name.clone() })
        }
        ButtonAction::PushToMute | ButtonAction::PushToTalk => {
            // When used as a press action (no hold context), behave as toggle
            state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::ToggleGlobalMute {
                input_id: s.input_id,
            })
        }
        ButtonAction::NextOutput => Some(DeviceEvent::NextOutput),
        ButtonAction::PrevOutput => Some(DeviceEvent::PrevOutput),
        ButtonAction::PageLeft => Some(DeviceEvent::PageLeft),
        ButtonAction::PageRight => Some(DeviceEvent::PageRight),
        ButtonAction::None => None,
    }
}

fn fire_press_action(
    event_tx: &tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
    action: &ButtonAction,
    button: Button,
    state: &Option<DisplayState>,
) {
    let state = match state {
        Some(s) => s,
        None => return,
    };
    let slot = button_slot(button);
    let current_output_id = state.outputs.get(state.current_output_index).map(|o| o.id).unwrap_or(0);
    if let Some(evt) = resolve_action_event(action, slot, current_output_id, state) {
        event_tx.send(evt).ok();
    }
}

fn fire_hold_action(
    event_tx: &tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
    action: &ButtonAction,
    button: Button,
    state: &Option<DisplayState>,
) {
    let state = match state {
        Some(s) => s,
        None => return,
    };
    let slot = button_slot(button);
    let current_output_id = state.outputs.get(state.current_output_index).map(|o| o.id).unwrap_or(0);

    // Special handling for PTM/PTT on hold trigger
    let event = match action {
        ButtonAction::PushToMute => {
            state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::SetGlobalMute {
                input_id: s.input_id,
                muted: true,
            })
        }
        ButtonAction::PushToTalk => {
            state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::SetGlobalMute {
                input_id: s.input_id,
                muted: false,
            })
        }
        other => resolve_action_event(other, slot, current_output_id, state),
    };

    if let Some(evt) = event {
        event_tx.send(evt).ok();
    }
}

fn fire_release_action(
    event_tx: &tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
    hold_action: &ButtonAction,
    captured_input_id: Option<u32>,
) {
    // Only PTM/PTT produce events on release
    let event = match hold_action {
        ButtonAction::PushToMute => {
            captured_input_id.map(|id| DeviceEvent::SetGlobalMute { input_id: id, muted: false })
        }
        ButtonAction::PushToTalk => {
            captured_input_id.map(|id| DeviceEvent::SetGlobalMute { input_id: id, muted: true })
        }
        _ => None,
    };
    if let Some(evt) = event {
        event_tx.send(evt).ok();
    }
}

fn handle_dials(
    event_tx: &tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
    dials: &[i8; 4],
    state: &Option<DisplayState>,
) {
    let state = match state {
        Some(s) => s,
        None => return,
    };

    let current_output_id = state
        .outputs
        .get(state.current_output_index)
        .map(|o| o.id)
        .unwrap_or(0);

    for (i, &delta) in dials.iter().enumerate() {
        if delta == 0 {
            continue;
        }
        if let Some(slot) = &state.visible_inputs[i] {
            event_tx
                .send(DeviceEvent::AdjustRouteVolume {
                    input_id: slot.input_id,
                    output_id: current_output_id,
                    delta,
                })
                .ok();
        }
    }
}

fn init_device(dev: &Device) -> anyhow::Result<()> {
    dev.write_command(&Command::Wake.to_bytes())?;
    dev.write_command(&Command::DisplayPower(true).to_bytes())?;
    let _ = dev.read(Duration::from_millis(50));
    dev.write_command(&Command::DisplayBrightness(40).to_bytes())?;
    dev.write_command(&Command::ButtonLedBrightness(255).to_bytes())?;
    Ok(())
}

fn shutdown_device(dev: &Device) {
    let off = Color { r: 0, g: 0, b: 0, a: 0 };
    let zones = [
        ButtonLighting::Dial1,
        ButtonLighting::Dial2,
        ButtonLighting::Dial3,
        ButtonLighting::Dial4,
        ButtonLighting::Mix,
        ButtonLighting::Left,
        ButtonLighting::Right,
    ];
    for zone in zones {
        let _ = dev.write_command(&Command::ButtonLedColor { zone, color: off }.to_bytes());
    }
    let _ = dev.write_command(&Command::ButtonLedBrightness(0).to_bytes());
    let _ = dev.write_command(&Command::DisplayPower(false).to_bytes());
}

fn send_full(dev: &Device, jpeg: &[u8]) -> anyhow::Result<()> {
    let enable = Command::DisplayPower(true).to_bytes();
    dev.write_command(&enable)?;
    let _ = dev.read(Duration::from_millis(5));
    for chunk in ImageChunker::new(jpeg, 0, 0) {
        dev.write_raw_timeout(&chunk, IMG_TIMEOUT)?;
    }
    Ok(())
}

fn send_patch(dev: &Device, patch: &Patch) -> anyhow::Result<()> {
    for chunk in ImageChunker::new(&patch.jpeg, patch.x, patch.y) {
        dev.write_raw_timeout(&chunk, IMG_TIMEOUT)?;
    }
    Ok(())
}

fn update_leds(dev: &Device, state: &DisplayState) -> anyhow::Result<()> {
    let dial_zones = [
        ButtonLighting::Dial1,
        ButtonLighting::Dial2,
        ButtonLighting::Dial3,
        ButtonLighting::Dial4,
    ];

    for (i, zone) in dial_zones.iter().enumerate() {
        let color = match &state.visible_inputs[i] {
            Some(slot) if slot.global_muted => Color {
                r: 180,
                g: 0,
                b: 0,
                a: 255,
            },
            Some(slot) if slot.route_muted => Color {
                r: 40,
                g: 40,
                b: 40,
                a: 255,
            },
            Some(slot) => {
                let (r, g, b) = slot.color;
                // 70% brightness
                Color {
                    r: (r as u16 * 70 / 100) as u8,
                    g: (g as u16 * 70 / 100) as u8,
                    b: (b as u16 * 70 / 100) as u8,
                    a: 255,
                }
            }
            None => Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        };
        dev.write_command(&Command::ButtonLedColor { zone: *zone, color }.to_bytes())?;
    }

    // Mix LED = current output color
    let mix_color = state
        .outputs
        .get(state.current_output_index)
        .map(|o| Color {
            r: o.color.0,
            g: o.color.1,
            b: o.color.2,
            a: 255,
        })
        .unwrap_or(Color { r: 0, g: 0, b: 0, a: 0 });
    dev.write_command(
        &Command::ButtonLedColor {
            zone: ButtonLighting::Mix,
            color: mix_color,
        }
        .to_bytes(),
    )?;

    // Page LEDs
    let left_color = if state.page > 0 {
        Color { r: 255, g: 255, b: 255, a: 255 }
    } else {
        Color { r: 30, g: 30, b: 30, a: 255 }
    };
    let right_color = if state.page + 1 < state.total_pages {
        Color { r: 255, g: 255, b: 255, a: 255 }
    } else {
        Color { r: 30, g: 30, b: 30, a: 255 }
    };

    dev.write_command(
        &Command::ButtonLedColor {
            zone: ButtonLighting::Left,
            color: left_color,
        }
        .to_bytes(),
    )?;
    dev.write_command(
        &Command::ButtonLedColor {
            zone: ButtonLighting::Right,
            color: right_color,
        }
        .to_bytes(),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mixctl_beacn_display::{DisplayState, OutputTab, SlotView};
    use mixctl_core::config_sections::{ButtonAction, ButtonMapping, ButtonMappings};

    fn test_display_state() -> DisplayState {
        DisplayState {
            current_output_index: 0,
            outputs: vec![
                OutputTab { id: 1, name: "Personal".into(), color: (142, 68, 173), is_current: true },
                OutputTab { id: 2, name: "Stream".into(), color: (52, 152, 219), is_current: false },
            ],
            visible_inputs: [
                Some(SlotView { input_id: 10, name: "System".into(), color: (74, 144, 217), volume: 80, route_muted: false, global_muted: false, level: None, streams: vec![] }),
                Some(SlotView { input_id: 11, name: "Game".into(), color: (231, 76, 60), volume: 60, route_muted: false, global_muted: false, level: None, streams: vec![] }),
                Some(SlotView { input_id: 12, name: "Music".into(), color: (46, 204, 113), volume: 100, route_muted: false, global_muted: false, level: None, streams: vec![] }),
                Some(SlotView { input_id: 13, name: "Chat".into(), color: (243, 156, 18), volume: 50, route_muted: false, global_muted: false, level: None, streams: vec![] }),
            ],
            page: 0,
            total_pages: 2,
        }
    }

    #[test]
    fn fire_press_route_mute() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Some(test_display_state());
        fire_press_action(&tx, &ButtonAction::ToggleRouteMute, Button::Dial1, &state);
        match rx.try_recv().unwrap() {
            DeviceEvent::ToggleRouteMute { input_id, output_id } => {
                assert_eq!(input_id, 10);
                assert_eq!(output_id, 1);
            }
            other => panic!("expected ToggleRouteMute, got {:?}", other),
        }
    }

    #[test]
    fn fire_press_global_mute() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Some(test_display_state());
        fire_press_action(&tx, &ButtonAction::ToggleGlobalMute, Button::Audience1, &state);
        match rx.try_recv().unwrap() {
            DeviceEvent::ToggleGlobalMute { input_id } => assert_eq!(input_id, 10),
            other => panic!("expected ToggleGlobalMute, got {:?}", other),
        }
    }

    #[test]
    fn fire_press_next_output() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Some(test_display_state());
        fire_press_action(&tx, &ButtonAction::NextOutput, Button::AudienceMix, &state);
        assert!(matches!(rx.try_recv().unwrap(), DeviceEvent::NextOutput));
    }

    #[test]
    fn fire_press_page_events() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Some(test_display_state());
        fire_press_action(&tx, &ButtonAction::PageLeft, Button::PageLeft, &state);
        assert!(matches!(rx.try_recv().unwrap(), DeviceEvent::PageLeft));
        fire_press_action(&tx, &ButtonAction::PageRight, Button::PageRight, &state);
        assert!(matches!(rx.try_recv().unwrap(), DeviceEvent::PageRight));
    }

    #[test]
    fn fire_press_dsp_toggles() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Some(test_display_state());
        fire_press_action(&tx, &ButtonAction::ToggleEq, Button::Dial2, &state);
        match rx.try_recv().unwrap() {
            DeviceEvent::ToggleEq { input_id } => assert_eq!(input_id, 11),
            other => panic!("expected ToggleEq, got {:?}", other),
        }
    }

    #[test]
    fn fire_hold_push_to_mute() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Some(test_display_state());
        fire_hold_action(&tx, &ButtonAction::PushToMute, Button::Audience1, &state);
        match rx.try_recv().unwrap() {
            DeviceEvent::SetGlobalMute { input_id, muted } => {
                assert_eq!(input_id, 10);
                assert!(muted);
            }
            other => panic!("expected SetGlobalMute, got {:?}", other),
        }
    }

    #[test]
    fn fire_release_push_to_mute() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        fire_release_action(&tx, &ButtonAction::PushToMute, Some(10));
        match rx.try_recv().unwrap() {
            DeviceEvent::SetGlobalMute { input_id, muted } => {
                assert_eq!(input_id, 10);
                assert!(!muted); // unmute on release
            }
            other => panic!("expected SetGlobalMute, got {:?}", other),
        }
    }

    #[test]
    fn fire_release_non_ptm_is_noop() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        fire_release_action(&tx, &ButtonAction::ToggleRouteMute, Some(10));
        assert!(rx.try_recv().is_err()); // no event
    }

    #[test]
    fn dial_rotation_sends_volume_adjust() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Some(test_display_state());
        let dials: [i8; 4] = [3, 0, 0, -2];
        handle_dials(&tx, &dials, &state);
        match rx.try_recv().unwrap() {
            DeviceEvent::AdjustRouteVolume { input_id, delta, .. } => {
                assert_eq!(input_id, 10);
                assert_eq!(delta, 3);
            }
            other => panic!("expected AdjustRouteVolume, got {:?}", other),
        }
        match rx.try_recv().unwrap() {
            DeviceEvent::AdjustRouteVolume { input_id, delta, .. } => {
                assert_eq!(input_id, 13);
                assert_eq!(delta, -2);
            }
            other => panic!("expected AdjustRouteVolume, got {:?}", other),
        }
        assert!(rx.try_recv().is_err());
    }
}
