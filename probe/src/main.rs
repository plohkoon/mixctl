mod usb;

use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mixctl_protocol::enums::ButtonLighting;
use mixctl_protocol::{parse_input, Color, Command, ImageChunker};
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "mixctl-probe", about = "Beacn Mix / Mix Create USB probe tool")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List Beacn Mix / Mix Create devices on the USB bus
    Discover,

    /// Open device, run init sequence, print version info
    Init,

    /// Send wake command
    Wake,

    /// Send poll, read + parse response, print dials/buttons
    Poll,

    /// Loop: poll + print input events until Ctrl-C
    Listen,

    /// Set display brightness (0-255)
    Brightness {
        value: u8,
    },

    /// Turn display on or off
    DisplayPower {
        #[arg(value_parser = parse_on_off)]
        state: bool,
    },

    /// Set button LED brightness (0-255)
    LedBrightness {
        value: u8,
    },

    /// Set button LED color
    LedColor {
        /// Zone: dial1, dial2, dial3, dial4, mix, left, right
        #[arg(value_parser = parse_zone)]
        zone: ButtonLighting,
        r: u8,
        g: u8,
        b: u8,
        #[arg(default_value = "255")]
        a: u8,
    },

    /// Send arbitrary hex bytes (for experimentation)
    Raw {
        /// Hex string (e.g. "00000005...")
        hex_data: String,
    },

    /// Transfer an image file to the display
    Image {
        path: String,
        #[arg(long, default_value = "0")]
        x: u16,
        #[arg(long, default_value = "0")]
        y: u16,
    },
}

fn parse_on_off(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        _ => Err(format!("expected on/off, got '{}'", s)),
    }
}

fn parse_zone(s: &str) -> Result<ButtonLighting, String> {
    match s.to_lowercase().as_str() {
        "dial1" | "0" => Ok(ButtonLighting::Dial1),
        "dial2" | "1" => Ok(ButtonLighting::Dial2),
        "dial3" | "2" => Ok(ButtonLighting::Dial3),
        "dial4" | "3" => Ok(ButtonLighting::Dial4),
        "mix" | "4" => Ok(ButtonLighting::Mix),
        "left" | "5" => Ok(ButtonLighting::Left),
        "right" | "6" => Ok(ButtonLighting::Right),
        _ => Err(format!(
            "unknown zone '{}' (expected dial1-4, mix, left, right)",
            s
        )),
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive(Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    match args.cmd {
        Cmd::Discover => {
            let devices = usb::discover()?;
            for dev in &devices {
                println!("{} @ bus {} addr {}", dev.device_type, dev.bus, dev.address);
            }
            println!("{} device(s) found", devices.len());
        }

        Cmd::Init => {
            let dev = usb::Device::open()?;
            println!("{} initialized successfully", dev.device_type);
        }

        Cmd::Wake => {
            let dev = usb::Device::open()?;
            let bytes = Command::Wake.to_bytes();
            dev.write_command(&bytes)?;
            println!("wake sent");
        }

        Cmd::Poll => {
            let dev = usb::Device::open()?;
            let bytes = Command::Poll.to_bytes();
            dev.write_command(&bytes)?;
            let resp = dev.read(Duration::from_secs(1))?;
            println!("raw response ({} bytes): {}", resp.len(), hex::encode(&resp));

            if resp.len() >= 10 {
                match parse_input(&resp) {
                    Ok(event) => {
                        println!("dials: {:?}", event.dials);
                        println!("buttons: {:?}", event.buttons_pressed);
                        println!("button mask: 0x{:04x}", event.button_mask);
                    }
                    Err(e) => println!("parse error: {}", e),
                }
            }
        }

        Cmd::Listen => {
            let dev = usb::Device::open()?;
            println!("listening for events (Ctrl-C to stop)...");

            loop {
                let bytes = Command::Poll.to_bytes();
                dev.write_command(&bytes)?;
                match dev.read(Duration::from_millis(100)) {
                    Ok(resp) => {
                        if resp.len() >= 10 {
                            match parse_input(&resp) {
                                Ok(event) => {
                                    let has_input = event.dials.iter().any(|&d| d != 0)
                                        || !event.buttons_pressed.is_empty();
                                    if has_input {
                                        if event.dials.iter().any(|&d| d != 0) {
                                            println!("dials: {:?}", event.dials);
                                        }
                                        if !event.buttons_pressed.is_empty() {
                                            println!(
                                                "buttons: {:?} (mask: 0x{:04x})",
                                                event.buttons_pressed, event.button_mask
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    info!("parse error: {}", e);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Timeout is normal — no data available
                    }
                }
            }
        }

        Cmd::Brightness { value } => {
            let dev = usb::Device::open()?;
            let bytes = Command::DisplayBrightness(value).to_bytes();
            dev.write_command(&bytes)?;
            println!("display brightness set to {}", value);
        }

        Cmd::DisplayPower { state } => {
            let dev = usb::Device::open()?;
            let bytes = Command::DisplayPower(state).to_bytes();
            dev.write_command(&bytes)?;
            println!("display power {}", if state { "on" } else { "off" });
        }

        Cmd::LedBrightness { value } => {
            let dev = usb::Device::open()?;
            let bytes = Command::ButtonLedBrightness(value).to_bytes();
            dev.write_command(&bytes)?;
            println!("LED brightness set to {}", value);
        }

        Cmd::LedColor { zone, r, g, b, a } => {
            let dev = usb::Device::open()?;
            let bytes = Command::ButtonLedColor {
                zone,
                color: Color { r, g, b, a },
            }
            .to_bytes();
            dev.write_command(&bytes)?;
            println!("LED color set for {:?}: rgb({}, {}, {}) a={}", zone, r, g, b, a);
        }

        Cmd::Raw { hex_data } => {
            let dev = usb::Device::open()?;
            let data = hex::decode(&hex_data).context("invalid hex string")?;
            let n = dev.write_raw(&data)?;
            println!("sent {} bytes", n);

            match dev.read(Duration::from_secs(1)) {
                Ok(resp) => {
                    println!("response ({} bytes): {}", resp.len(), hex::encode(&resp));
                }
                Err(_) => {
                    println!("no response (timeout)");
                }
            }
        }

        Cmd::Image { path, x, y } => {
            let dev = usb::Device::open()?;
            let data = std::fs::read(&path)
                .with_context(|| format!("failed to read image file: {}", path))?;
            println!("sending {} bytes to display at ({}, {})", data.len(), x, y);

            let mut count = 0;
            for chunk in ImageChunker::new(&data, x, y) {
                dev.write_raw(&chunk)?;
                count += 1;
            }
            println!("sent {} packets", count);
        }
    }

    Ok(())
}
