use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use mixctl_beacn_display::{DisplayLayout, DisplayState, Patch};
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
    layout: Box<dyn DisplayLayout>,
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
            &*layout,
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
    layout: &dyn DisplayLayout,
) -> DisconnectReason {
    let mut current_state: Option<DisplayState> = None;
    let mut prev_state: Option<DisplayState> = None;
    let mut needs_full_redraw = true;
    let mut prev_button_mask: u16 = 0;

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
                Ok(DeviceCommand::Shutdown) => {
                    return DisconnectReason::CommandShutdown;
                }
                Err(_) => break,
            }
        }

        // Render updates if state changed
        if let Some(ref state) = current_state {
            if needs_full_redraw {
                let jpeg = layout.render_full(state);
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
                    let patches = layout.render_diff(prev, state);
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
                    // Rising-edge detection: only fire for buttons newly pressed this cycle
                    let newly_pressed = event.button_mask & !prev_button_mask;
                    prev_button_mask = event.button_mask;

                    if newly_pressed != 0 {
                        let new_buttons: Vec<Button> = Button::ALL
                            .iter()
                            .filter(|b| b.is_pressed(newly_pressed))
                            .copied()
                            .collect();
                        handle_buttons(event_tx, &new_buttons, &current_state);
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

fn handle_buttons(
    event_tx: &tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
    buttons: &[Button],
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

    for button in buttons {
        let event = match button {
            Button::Dial1 => slot_toggle_route_mute(state, 0, current_output_id),
            Button::Dial2 => slot_toggle_route_mute(state, 1, current_output_id),
            Button::Dial3 => slot_toggle_route_mute(state, 2, current_output_id),
            Button::Dial4 => slot_toggle_route_mute(state, 3, current_output_id),
            Button::Audience1 => slot_toggle_global_mute(state, 0),
            Button::Audience2 => slot_toggle_global_mute(state, 1),
            Button::Audience3 => slot_toggle_global_mute(state, 2),
            Button::Audience4 => slot_toggle_global_mute(state, 3),
            Button::AudienceMix => Some(DeviceEvent::NextOutput),
            Button::PageLeft => Some(DeviceEvent::PageLeft),
            Button::PageRight => Some(DeviceEvent::PageRight),
        };
        if let Some(evt) = event {
            event_tx.send(evt).ok();
        }
    }
}

fn slot_toggle_route_mute(state: &DisplayState, slot: usize, output_id: u32) -> Option<DeviceEvent> {
    state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::ToggleRouteMute {
        input_id: s.input_id,
        output_id,
    })
}

fn slot_toggle_global_mute(state: &DisplayState, slot: usize) -> Option<DeviceEvent> {
    state.visible_inputs[slot].as_ref().map(|s| DeviceEvent::ToggleGlobalMute {
        input_id: s.input_id,
    })
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
