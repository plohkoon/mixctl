mod usb;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use image::{ImageBuffer, Rgb, codecs::jpeg::JpegEncoder};
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

    /// Turn display on or off, or dump raw poll data with no argument
    DisplayPower {
        /// on/off (omit to query device)
        #[arg(value_parser = parse_on_off)]
        state: Option<bool>,
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

    /// Scan command space: try byte combinations and log responses
    Scan {
        /// First byte to scan (0x00-0xff)
        #[arg(long, default_value = "0", value_parser = parse_hex_u8)]
        b0: u8,
        /// Second byte range start (0x00-0xff)
        #[arg(long, default_value = "0", value_parser = parse_hex_u8)]
        b1_start: u8,
        /// Second byte range end inclusive (0x00-0xff)
        #[arg(long, default_value = "ff", value_parser = parse_hex_u8)]
        b1_end: u8,
        /// Fixed bytes 2-3 (the "subcommand" field, e.g. "0004" or "00f1")
        #[arg(long, default_value = "0004", value_parser = parse_hex_u16)]
        sub: u16,
        /// Read timeout in ms
        #[arg(long, default_value = "200")]
        timeout_ms: u64,
    },

    /// Interactive HSLA color picker: 4 dials control H/S/L/A sliders on display, buttons show the color
    Debug,

    /// Transfer an image file to the display
    Image {
        path: String,
        #[arg(long, default_value = "0")]
        x: u32,
        #[arg(long, default_value = "0")]
        y: u32,
    },
}

fn parse_hex_u8(s: &str) -> Result<u8, String> {
    u8::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16)
        .map_err(|e| format!("invalid hex byte '{}': {}", s, e))
}

fn parse_hex_u16(s: &str) -> Result<u16, String> {
    u16::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16)
        .map_err(|e| format!("invalid hex u16 '{}': {}", s, e))
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

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s == 0.0 {
        let v = (l * 255.0) as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue_to_rgb = |t: f32| -> f32 {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let r = (hue_to_rgb(h + 1.0 / 3.0) * 255.0) as u8;
    let g = (hue_to_rgb(h) * 255.0) as u8;
    let b = (hue_to_rgb(h - 1.0 / 3.0) * 255.0) as u8;
    (r, g, b)
}

// Layout: 2x2 grid of horizontal sliders with labels below
const DISPLAY_W: u32 = 800;
const DISPLAY_H: u32 = 480;
const PAD: u32 = 30;               // outer padding
const SWATCH_H: u32 = 60;
const SWATCH_Y: u32 = PAD;
const GRID_GAP_X: u32 = 30;        // horizontal gap between columns
const GRID_GAP_Y: u32 = 20;        // vertical gap between rows
const CELL_W: u32 = (DISPLAY_W - 2 * PAD - GRID_GAP_X) / 2;
const BAR_H: u32 = 60;             // height of the colored bar
const LABEL_H: u32 = 24;           // height for label text below bar
const CELL_H: u32 = BAR_H + LABEL_H;
const GRID_TOP: u32 = SWATCH_Y + SWATCH_H + 24;

/// Returns (x, y) of the top-left of cell i (0=top-left, 1=top-right, 2=bottom-left, 3=bottom-right)
fn cell_origin(i: u32) -> (u32, u32) {
    let col = i % 2;
    let row = i / 2;
    let x = PAD + col * (CELL_W + GRID_GAP_X);
    let y = GRID_TOP + row * (CELL_H + GRID_GAP_Y);
    (x, y)
}


const SWATCH_W: u32 = DISPLAY_W - 2 * PAD;
const BAR_INSET: u32 = 3; // border inset for fill area

fn encode_jpeg(img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Vec<u8> {
    encode_jpeg_q(img, 80)
}

fn encode_jpeg_fast(img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Vec<u8> {
    encode_jpeg_q(img, 50)
}

fn encode_jpeg_q(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, quality: u8) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    encoder
        .encode(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8)
        .expect("jpeg encode failed");
    buf
}

/// Pre-rendered cell template: background + border, no fill or label
fn make_cell_template() -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut img = ImageBuffer::new(CELL_W, CELL_H);
    let bg = Rgb([15, 15, 20]);
    let track = Rgb([30, 30, 35]);
    let border = Rgb([60, 60, 65]);

    // Fill background
    for pixel in img.pixels_mut() { *pixel = bg; }

    // Bar track
    for y in 0..BAR_H {
        for x in 0..CELL_W {
            img.put_pixel(x, y, track);
        }
    }

    // Border
    for y in 0..BAR_H {
        img.put_pixel(0, y, border);
        img.put_pixel(CELL_W - 1, y, border);
    }
    for x in 0..CELL_W {
        img.put_pixel(x, 0, border);
        img.put_pixel(x, BAR_H - 1, border);
    }

    img
}

/// Pre-rendered swatch template: border only
fn make_swatch_template() -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut img = ImageBuffer::new(SWATCH_W, SWATCH_H);
    let border = Rgb([60, 60, 65]);

    for pixel in img.pixels_mut() { *pixel = Rgb([0, 0, 0]); }

    // Border
    for x in 0..SWATCH_W {
        img.put_pixel(x, 0, border);
        img.put_pixel(x, SWATCH_H - 1, border);
    }
    for y in 0..SWATCH_H {
        img.put_pixel(0, y, border);
        img.put_pixel(SWATCH_W - 1, y, border);
    }

    img
}

/// Render a cell patch from template: clone, paint fill + label, encode
fn render_cell_patch(template: &ImageBuffer<Rgb<u8>, Vec<u8>>, page: u8, index: u32, val: f32) -> Vec<u8> {
    let mut img = template.clone();
    let bar_inner = CELL_W - 2 * BAR_INSET;
    let fill_w = (val * bar_inner as f32) as u32;

    // Paint fill: fixed gradient clipped at current value
    for x in 0..fill_w.min(bar_inner) {
        let frac = x as f32 / bar_inner as f32;
        let (cr, cg, cb) = hslider_fill_color(page, index, frac);
        let color = Rgb([cr, cg, cb]);
        for y in BAR_INSET..BAR_H - BAR_INSET {
            img.put_pixel(x + BAR_INSET, y, color);
        }
    }

    // Label centered below bar
    let label = slider_label(page, index, val);
    let text_w = label.len() as u32 * 12;
    let text_x = (CELL_W.saturating_sub(text_w)) / 2;
    draw_text(&mut img, text_x, BAR_H + 4, &label, Rgb([180, 180, 180]));

    encode_jpeg_fast(&img)
}

/// Render swatch patch from template: clone, fill color scaled by brightness, encode
fn render_swatch_patch(template: &ImageBuffer<Rgb<u8>, Vec<u8>>, rgba: [u8; 4]) -> Vec<u8> {
    let mut img = template.clone();
    let br = rgba[3] as f32 / 255.0;
    let color = Rgb([
        (rgba[0] as f32 * br) as u8,
        (rgba[1] as f32 * br) as u8,
        (rgba[2] as f32 * br) as u8,
    ]);
    for y in 1..SWATCH_H - 1 {
        for x in 1..SWATCH_W - 1 {
            img.put_pixel(x, y, color);
        }
    }
    encode_jpeg_fast(&img)
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if (max - g).abs() < f32::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}


/// Render a slider into a band image, only the rows within y_min..y_max (absolute coords)
/// Render a full 800x480 frame for a given page
fn render_full_page(page: u8, values: [f32; 4], rgba: [u8; 4]) -> Vec<u8> {
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(DISPLAY_W, DISPLAY_H);
    let bg = Rgb([15, 15, 20]);
    for pixel in img.pixels_mut() { *pixel = bg; }

    // Swatch (color scaled by brightness)
    let br = rgba[3] as f32 / 255.0;
    let color = Rgb([
        (rgba[0] as f32 * br) as u8,
        (rgba[1] as f32 * br) as u8,
        (rgba[2] as f32 * br) as u8,
    ]);
    let border = Rgb([60, 60, 65]);
    for y in SWATCH_Y..SWATCH_Y + SWATCH_H {
        for x in PAD..DISPLAY_W - PAD {
            img.put_pixel(x, y, color);
        }
    }
    for x in PAD..DISPLAY_W - PAD {
        img.put_pixel(x, SWATCH_Y, border);
        img.put_pixel(x, SWATCH_Y + SWATCH_H - 1, border);
    }
    for y in SWATCH_Y..SWATCH_Y + SWATCH_H {
        img.put_pixel(PAD, y, border);
        img.put_pixel(DISPLAY_W - PAD - 1, y, border);
    }

    // Sliders
    for i in 0..4u32 {
        draw_hslider(&mut img, page, i, values[i as usize]);
    }
    encode_jpeg(&img)
}


/// Draw a horizontal slider into a full-frame image
fn draw_hslider(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, page: u8, index: u32, val: f32) {
    let (cx, cy) = cell_origin(index);
    let bar_w = CELL_W;
    let bar_inner = bar_w - 2 * BAR_INSET;
    let fill_w = (val * bar_inner as f32) as u32;

    // Bar background
    for y in cy..cy + BAR_H {
        for x in cx..cx + bar_w {
            img.put_pixel(x, y, Rgb([30, 30, 35]));
        }
    }

    // Filled portion: fixed gradient clipped at current value
    for x in 0..fill_w.min(bar_inner) {
        let frac = x as f32 / bar_inner as f32;
        let (cr, cg, cb) = hslider_fill_color(page, index, frac);
        for y in cy + BAR_INSET..cy + BAR_H - BAR_INSET {
            img.put_pixel(cx + BAR_INSET + x, y, Rgb([cr, cg, cb]));
        }
    }

    // Border
    let border = Rgb([60, 60, 65]);
    for y in cy..cy + BAR_H {
        img.put_pixel(cx, y, border);
        img.put_pixel(cx + bar_w - 1, y, border);
    }
    for x in cx..cx + bar_w {
        img.put_pixel(x, cy, border);
        img.put_pixel(x, cy + BAR_H - 1, border);
    }

    // Label centered below bar
    let label = slider_label(page, index, val);
    let text_w = label.len() as u32 * 12; // 6px glyph * 2 scale
    let text_x = cx + (bar_w.saturating_sub(text_w)) / 2;
    let text_y = cy + BAR_H + 4;
    draw_text(img, text_x, text_y, &label, Rgb([180, 180, 180]));
}


/// Fill color for a horizontal slider (left-to-right)
fn hslider_fill_color(page: u8, index: u32, frac: f32) -> (u8, u8, u8) {
    let b = 0.4 + 0.6 * frac;
    match page {
        0 => match index {
            0 => hsl_to_rgb(frac, 1.0, 0.5),  // Hue: full spectrum left to right
            1 => ((80.0 * b) as u8, (220.0 * b) as u8, (80.0 * b) as u8),
            2 => ((60.0 * b) as u8, (120.0 * b) as u8, (255.0 * b) as u8),
            _ => ((255.0 * b) as u8, (220.0 * b) as u8, (180.0 * b) as u8),
        },
        1 => match index {
            0 => ((255.0 * b) as u8, (40.0 * b) as u8, (40.0 * b) as u8),
            1 => ((40.0 * b) as u8, (255.0 * b) as u8, (40.0 * b) as u8),
            2 => ((40.0 * b) as u8, (80.0 * b) as u8, (255.0 * b) as u8),
            _ => ((255.0 * b) as u8, (240.0 * b) as u8, (200.0 * b) as u8),
        },
        _ => unreachable!(),
    }
}

/// Get label for a slider value
fn slider_label(page: u8, index: u32, val: f32) -> String {
    match page {
        0 => match index {
            0 => format!("H {}", (val * 360.0) as u32),
            1 => format!("S {}%", (val * 100.0) as u32),
            2 => format!("L {}%", (val * 100.0) as u32),
            _ => format!("A {}%", (val * 100.0) as u32),
        },
        1 => {
            let v = (val * 255.0) as u32;
            match index {
                0 => format!("R {}", v),
                1 => format!("G {}", v),
                2 => format!("B {}", v),
                _ => format!("Br {}", v),
            }
        }
        _ => unreachable!(),
    }
}

// Tiny 5x7 bitmap font renderer
fn draw_text(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, x: u32, y: u32, text: &str, color: Rgb<u8>) {
    const GLYPHS: &[(char, [u8; 7])] = &[
        ('0', [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
        ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
        ('2', [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111]),
        ('3', [0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110]),
        ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
        ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
        ('6', [0b01110, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b01110]),
        ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
        ('8', [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
        ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b10001, 0b01110]),
        ('H', [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
        ('S', [0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110]),
        ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
        ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
        ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
        ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110]),
        ('B', [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
        ('r', [0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000]),
        ('%', [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011]),
        (' ', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000]),
    ];

    let scale = 2u32;
    let mut cx = x;
    for ch in text.chars() {
        if let Some((_, glyph)) = GLYPHS.iter().find(|(c, _)| *c == ch) {
            for (row, &bits) in glyph.iter().enumerate() {
                for col in 0..5u32 {
                    if bits & (1 << (4 - col)) != 0 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = cx + col * scale + sx;
                                let py = y + row as u32 * scale + sy;
                                if px < img.width() && py < img.height() {
                                    img.put_pixel(px, py, color);
                                }
                            }
                        }
                    }
                }
            }
        }
        cx += 6 * scale;
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

        Cmd::DisplayPower { state: Some(on) } => {
            let dev = usb::Device::open()?;
            let bytes = Command::DisplayPower(on).to_bytes();
            dev.write_command(&bytes)?;
            println!("display power {}", if on { "on" } else { "off" });
        }

        Cmd::DisplayPower { state: None } => {
            let dev = usb::Device::open()?;
            // Send a poll and dump the full raw response for analysis
            let bytes = Command::Poll.to_bytes();
            dev.write_command(&bytes)?;
            match dev.read(Duration::from_secs(1)) {
                Ok(resp) => {
                    println!("poll response ({} bytes):", resp.len());
                    for (i, chunk) in resp.chunks(16).enumerate() {
                        print!("  {:04x}:  ", i * 16);
                        for (j, byte) in chunk.iter().enumerate() {
                            if j > 0 { print!(" "); }
                            print!("{:02x}", byte);
                        }
                        print!("  ");
                        for byte in chunk {
                            let c = if byte.is_ascii_graphic() || *byte == b' ' { *byte as char } else { '.' };
                            print!("{}", c);
                        }
                        println!();
                    }
                }
                Err(e) => println!("no response: {}", e),
            }
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

        Cmd::Scan { b0, b1_start, b1_end, sub, timeout_ms } => {
            let dev = usb::Device::open()?;
            let sub_bytes = sub.to_be_bytes();
            let timeout = Duration::from_millis(timeout_ms);
            let mut found = 0u32;

            // Known commands for reference
            println!("scanning b0=0x{:02x}, b1=0x{:02x}..0x{:02x}, sub=0x{:04x}", b0, b1_start, b1_end, sub);
            println!("known: poll=00,00,00,05  wake=00,00,00,F1  init=00,00,00,01");
            println!("known: brightness=00,00  display_power=00,01  led_bright=01,07  led_color=01,00-06");
            println!("---");

            for b1 in b1_start..=b1_end {
                let cmd = [b0, b1, sub_bytes[0], sub_bytes[1], 0x00, 0x00, 0x00, 0x00];
                if let Err(e) = dev.write_command(&cmd) {
                    println!("[{:02x} {:02x}] write error: {}", b0, b1, e);
                    continue;
                }

                match dev.read(timeout) {
                    Ok(resp) => {
                        // Skip if it looks like a standard poll echo (01 00 00 02 + zeros)
                        let is_poll_echo = resp.len() >= 4
                            && resp[0] == 0x01 && resp[1] == 0x00
                            && resp[2] == 0x00 && resp[3] == 0x02
                            && resp[4..].iter().all(|&b| b == 0);
                        if is_poll_echo {
                            continue;
                        }

                        found += 1;
                        print!("[{:02x} {:02x}] response ({:2} bytes): ", b0, b1, resp.len());
                        for byte in &resp {
                            print!("{:02x} ", byte);
                        }
                        print!(" |");
                        for byte in &resp {
                            let c = if byte.is_ascii_graphic() || *byte == b' ' { *byte as char } else { '.' };
                            print!("{}", c);
                        }
                        println!("|");
                    }
                    Err(_) => {
                        // Timeout — no response for this command
                    }
                }
            }

            println!("---");
            println!("scan complete: {} interesting response(s)", found);
        }

        Cmd::Debug => {
            let dev = usb::Device::open()?;
            let img_timeout = Duration::from_millis(100);

            // Ctrl-C handler
            let running = Arc::new(AtomicBool::new(true));
            let r = running.clone();
            ctrlc::set_handler(move || r.store(false, Ordering::SeqCst))
                .context("failed to set Ctrl-C handler")?;

            // Init: wake, display on, brightness up, LEDs on
            dev.write_command(&Command::Wake.to_bytes())?;
            dev.write_command(&Command::DisplayPower(true).to_bytes())?;
            let _ = dev.read(img_timeout);
            dev.write_command(&Command::DisplayBrightness(40).to_bytes())?;
            dev.write_command(&Command::ButtonLedBrightness(255).to_bytes())?;

            // Phase 4: Pre-render templates at startup
            let cell_template = make_cell_template();
            let swatch_template = make_swatch_template();

            // Phase 1: send_image without enable+ack (only send data chunks)
            // The enable was already sent once during init above.
            let send_patch = |dev: &usb::Device, jpeg: &[u8], x: u32, y: u32| -> Result<()> {
                for chunk in ImageChunker::new(jpeg, x, y) {
                    dev.write_raw_timeout(&chunk, img_timeout)?;
                }
                Ok(())
            };

            // Full frame send (used for init and page switches — needs enable)
            let send_full = |dev: &usb::Device, jpeg: &[u8]| -> Result<()> {
                let enable = Command::DisplayPower(true).to_bytes();
                dev.write_command(&enable)?;
                let _ = dev.read(Duration::from_millis(5));
                for chunk in ImageChunker::new(jpeg, 0, 0) {
                    dev.write_raw_timeout(&chunk, img_timeout)?;
                }
                Ok(())
            };

            let update_leds = |dev: &usb::Device, r: u8, g: u8, b: u8, brightness: u8,
                               page: u8| -> Result<()> {
                // Brightness is controlled by the dedicated brightness command
                dev.write_command(&Command::ButtonLedBrightness(brightness).to_bytes())?;
                let color = Color { r, g, b, a: 255 };
                let dial_zones = [
                    ButtonLighting::Dial1, ButtonLighting::Dial2,
                    ButtonLighting::Dial3, ButtonLighting::Dial4,
                    ButtonLighting::Mix,
                ];
                for zone in dial_zones {
                    dev.write_command(&Command::ButtonLedColor { zone, color }.to_bytes())?;
                }
                let dim = Color { r: 30, g: 30, b: 30, a: 255 };
                let bright = Color { r: 255, g: 255, b: 255, a: 255 };
                let left_color = if page > 0 { bright } else { dim };
                let right_color = if page < NUM_PAGES - 1 { bright } else { dim };
                dev.write_command(&Command::ButtonLedColor {
                    zone: ButtonLighting::Left, color: left_color
                }.to_bytes())?;
                dev.write_command(&Command::ButtonLedColor {
                    zone: ButtonLighting::Right, color: right_color
                }.to_bytes())?;
                Ok(())
            };

            let sync_rgba_from_hsla = |hsla: &[f32; 4], rgba: &mut [u8; 4]| {
                let (r, g, b) = hsl_to_rgb(hsla[0], hsla[1], hsla[2]);
                rgba[0] = r;
                rgba[1] = g;
                rgba[2] = b;
                rgba[3] = (hsla[3] * 255.0) as u8;
            };

            let sync_hsla_from_rgba = |rgba: &[u8; 4], hsla: &mut [f32; 4]| {
                let (h, s, l) = rgb_to_hsl(rgba[0], rgba[1], rgba[2]);
                hsla[0] = h;
                hsla[1] = s;
                hsla[2] = l;
                hsla[3] = rgba[3] as f32 / 255.0;
            };

            let page_values = |page: u8, hsla: &[f32; 4], rgba: &[u8; 4]| -> [f32; 4] {
                match page {
                    0 => *hsla,
                    1 => [
                        rgba[0] as f32 / 255.0,
                        rgba[1] as f32 / 255.0,
                        rgba[2] as f32 / 255.0,
                        rgba[3] as f32 / 255.0,
                    ],
                    _ => unreachable!(),
                }
            };

            // Color state
            let mut hsla = [0.5_f32, 1.0, 0.5, 1.0];
            let mut rgba = [128_u8, 0, 128, 255];
            let mut page: u8 = 0;
            const NUM_PAGES: u8 = 2;

            // Initial sync & render
            sync_rgba_from_hsla(&hsla, &mut rgba);
            let vals = page_values(page, &hsla, &rgba);
            update_leds(&dev, rgba[0], rgba[1], rgba[2], rgba[3], page)?;
            let full = render_full_page(page, vals, rgba);
            send_full(&dev, &full)?;
            println!("debug mode: page 1/2 HSLA — left/right to switch, dials to adjust (Ctrl-C to stop)");

            let mut prev_values = vals;
            // Phase 5: track prev_rgba for swatch dedup
            let mut prev_rgba = rgba;

            while running.load(Ordering::SeqCst) {
                dev.write_command(&Command::Poll.to_bytes())?;
                match dev.read(Duration::from_millis(50)) {
                    Ok(resp) if resp.len() >= 10 => {
                        if let Ok(event) = parse_input(&resp) {
                            // Handle page switches
                            if event.buttons_pressed.contains(&mixctl_protocol::Button::PageRight)
                                && page < NUM_PAGES - 1
                            {
                                page += 1;
                                let vals = page_values(page, &hsla, &rgba);
                                prev_values = vals;
                                prev_rgba = rgba;
                                update_leds(&dev, rgba[0], rgba[1], rgba[2], rgba[3], page)?;
                                let full = render_full_page(page, vals, rgba);
                                send_full(&dev, &full)?;
                                println!("page {}/{} {}",
                                    page + 1, NUM_PAGES,
                                    if page == 0 { "HSLA" } else { "RGB+B" });
                                continue;
                            }
                            if event.buttons_pressed.contains(&mixctl_protocol::Button::PageLeft)
                                && page > 0
                            {
                                page -= 1;
                                let vals = page_values(page, &hsla, &rgba);
                                prev_values = vals;
                                prev_rgba = rgba;
                                update_leds(&dev, rgba[0], rgba[1], rgba[2], rgba[3], page)?;
                                let full = render_full_page(page, vals, rgba);
                                send_full(&dev, &full)?;
                                println!("page {}/{} {}",
                                    page + 1, NUM_PAGES,
                                    if page == 0 { "HSLA" } else { "RGB+B" });
                                continue;
                            }

                            let dials = event.dials;
                            if dials.iter().all(|&d| d == 0) {
                                continue;
                            }

                            match page {
                                0 => {
                                    if dials[0] != 0 {
                                        hsla[0] = (hsla[0] + dials[0] as f32 / 360.0).rem_euclid(1.0);
                                    }
                                    if dials[1] != 0 {
                                        hsla[1] = (hsla[1] + dials[1] as f32 / 100.0).clamp(0.0, 1.0);
                                    }
                                    if dials[2] != 0 {
                                        hsla[2] = (hsla[2] + dials[2] as f32 / 100.0).clamp(0.0, 1.0);
                                    }
                                    if dials[3] != 0 {
                                        hsla[3] = (hsla[3] + dials[3] as f32 / 100.0).clamp(0.0, 1.0);
                                    }
                                    sync_rgba_from_hsla(&hsla, &mut rgba);
                                }
                                1 => {
                                    if dials[0] != 0 {
                                        rgba[0] = (rgba[0] as i16 + dials[0] as i16).clamp(0, 255) as u8;
                                    }
                                    if dials[1] != 0 {
                                        rgba[1] = (rgba[1] as i16 + dials[1] as i16).clamp(0, 255) as u8;
                                    }
                                    if dials[2] != 0 {
                                        rgba[2] = (rgba[2] as i16 + dials[2] as i16).clamp(0, 255) as u8;
                                    }
                                    if dials[3] != 0 {
                                        rgba[3] = (rgba[3] as i16 + dials[3] as i16).clamp(0, 255) as u8;
                                    }
                                    sync_hsla_from_rgba(&rgba, &mut hsla);
                                }
                                _ => {}
                            }

                            println!(
                                "H:{:3} S:{:3}% L:{:3}% A:{:3}% | R:{:3} G:{:3} B:{:3} Br:{:3}",
                                (hsla[0] * 360.0) as u32,
                                (hsla[1] * 100.0) as u32,
                                (hsla[2] * 100.0) as u32,
                                (hsla[3] * 100.0) as u32,
                                rgba[0], rgba[1], rgba[2], rgba[3]
                            );

                            update_leds(&dev, rgba[0], rgba[1], rgba[2], rgba[3], page)?;

                            // Phase 2 + 5: send individual cell-sized patches
                            let new_values = page_values(page, &hsla, &rgba);

                            // Phase 5: swatch only if actual RGBA changed
                            if rgba != prev_rgba {
                                let jpeg = render_swatch_patch(&swatch_template, rgba);
                                send_patch(&dev, &jpeg, PAD, SWATCH_Y)?;
                            }

                            // Phase 2: only dirty cells, each as a narrow patch
                            for i in 0..4usize {
                                if (prev_values[i] - new_values[i]).abs() > f32::EPSILON {
                                    let jpeg = render_cell_patch(
                                        &cell_template, page, i as u32, new_values[i],
                                    );
                                    let (cx, cy) = cell_origin(i as u32);
                                    send_patch(&dev, &jpeg, cx, cy)?;
                                }
                            }

                            prev_values = new_values;
                            prev_rgba = rgba;
                        }
                    }
                    _ => {}
                }
            }

            // Shutdown: turn off LEDs and display
            println!("\nshutting down...");
            let off = Color { r: 0, g: 0, b: 0, a: 0 };
            let zones = [
                ButtonLighting::Dial1, ButtonLighting::Dial2,
                ButtonLighting::Dial3, ButtonLighting::Dial4,
                ButtonLighting::Mix, ButtonLighting::Left, ButtonLighting::Right,
            ];
            for zone in zones {
                let _ = dev.write_command(&Command::ButtonLedColor { zone, color: off }.to_bytes());
            }
            let _ = dev.write_command(&Command::ButtonLedBrightness(0).to_bytes());
            let _ = dev.write_command(&Command::DisplayPower(false).to_bytes());
            println!("done");
        }

        Cmd::Image { path, x, y } => {
            let dev = usb::Device::open()?;
            let data = std::fs::read(&path)
                .with_context(|| format!("failed to read image file: {}", path))?;
            println!("sending {} bytes to display at ({}, {})", data.len(), x, y);

            let img_timeout = Duration::from_millis(100);

            // Send enable/reset command before image transfer
            let enable = Command::DisplayPower(true).to_bytes();
            dev.write_command(&enable)?;

            // Drain any ack response
            let _ = dev.read(img_timeout);

            let mut count = 0;
            for chunk in ImageChunker::new(&data, x, y) {
                dev.write_raw_timeout(&chunk, img_timeout)?;
                count += 1;
            }
            println!("sent {} packets", count);
        }
    }

    Ok(())
}
