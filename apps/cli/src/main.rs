use anyhow::Result;
use clap::{Parser, Subcommand};
use mixctl_core::config_sections::{BeacnConfig, ButtonAction, ButtonMapping, ButtonMappings};
use mixctl_core::dbus::MixCtlProxy;
use zbus::Connection;

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!("expected true/false, got '{s}'")),
    }
}

/// Parse capabilities JSON into a human-readable summary line.
fn summarize_capabilities(json: &str) -> String {
    let caps: Vec<serde_json::Value> = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return "(invalid capabilities)".to_string(),
    };
    let mut parts = Vec::new();
    for cap in &caps {
        if let Some(obj) = cap.as_object() {
            if let Some(fader) = obj.get("Fader") {
                let count = fader.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                parts.push(format!("{count} faders"));
            } else if let Some(button) = obj.get("Button") {
                let count = button.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                parts.push(format!("{count} buttons"));
            } else if let Some(screen) = obj.get("Screen") {
                let w = screen.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
                let h = screen.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                parts.push(format!("screen ({w}x{h})"));
            } else if let Some(led) = obj.get("Led") {
                let count = led.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                parts.push(format!("{count} LEDs"));
            } else if let Some(meter) = obj.get("Meter") {
                let count = meter.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                parts.push(format!("{count} meters"));
            }
        }
    }
    if parts.is_empty() {
        "(no capabilities)".to_string()
    } else {
        parts.join(", ")
    }
}

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ping the daemon
    Ping,
    /// Input management
    Input {
        #[command(subcommand)]
        cmd: InputCmd,
    },
    /// Output management
    Output {
        #[command(subcommand)]
        cmd: OutputCmd,
    },
    /// Route management (input→output routing)
    Route {
        #[command(subcommand)]
        cmd: RouteCmd,
    },
    /// Listen for daemon signals (runs until interrupted)
    Listen {
        #[command(subcommand)]
        cmd: ListenCmd,
    },
    /// Audio stream management
    Stream {
        #[command(subcommand)]
        cmd: StreamCmd,
    },
    /// App rule management (auto-assign streams to inputs)
    Rule {
        #[command(subcommand)]
        cmd: RuleCmd,
    },
    /// Capture device management (hardware inputs)
    Capture {
        #[command(subcommand)]
        cmd: CaptureCmd,
    },
    /// Config section management
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Level monitoring
    Level {
        #[command(subcommand)]
        cmd: LevelCmd,
    },
    /// List connected components
    Component {
        #[command(subcommand)]
        cmd: ComponentCmd,
    },
    /// Playback device management
    Playback {
        #[command(subcommand)]
        cmd: PlaybackCmd,
    },
    /// Profile management (save/load routing configurations)
    Profile {
        #[command(subcommand)]
        cmd: ProfileCmd,
    },
    /// Manage custom inputs (non-audio dial controls)
    #[command(name = "custom-input")]
    CustomInput {
        #[command(subcommand)]
        cmd: CustomInputCmd,
    },
    /// Device adapter management
    Adapter {
        #[command(subcommand)]
        cmd: AdapterCmd,
    },
    /// Audio status
    Status,
    /// Manage Beacn hardware device
    Beacn {
        #[command(subcommand)]
        cmd: BeacnCmd,
    },
}

#[derive(Subcommand)]
enum ProfileCmd {
    /// List saved profiles
    List,
    /// Save current configuration as a named profile
    Save { name: String },
    /// Load a saved profile (applies live)
    Load { name: String },
    /// Delete a saved profile
    Delete { name: String },
}

#[derive(Subcommand)]
enum BeacnCmd {
    /// Show full beacn config
    Status,
    /// List all 11 button mappings (press + hold)
    Buttons,
    /// Set a button's press or hold action
    Button {
        /// Button name (dial1-4, audience1-4, mix, page-left, page-right)
        name: String,
        /// Trigger type: "press" or "hold"
        trigger: String,
        /// Action (snake_case, e.g. toggle_route_mute, mute_output:5)
        action: String,
    },
    /// Set a beacn config value
    Set {
        /// Config key: layout, dial-sensitivity, hold-threshold, display-brightness, led-brightness
        key: String,
        /// Value to set
        value: String,
    },
}

fn parse_button_action(s: &str) -> Result<ButtonAction, String> {
    match s {
        "toggle_route_mute" => Ok(ButtonAction::ToggleRouteMute),
        "toggle_global_mute" => Ok(ButtonAction::ToggleGlobalMute),
        "mute_all_outputs" => Ok(ButtonAction::MuteAllOutputs),
        "toggle_eq" => Ok(ButtonAction::ToggleEq),
        "toggle_gate" => Ok(ButtonAction::ToggleGate),
        "toggle_deesser" => Ok(ButtonAction::ToggleDeesser),
        "toggle_compressor" => Ok(ButtonAction::ToggleCompressor),
        "toggle_limiter" => Ok(ButtonAction::ToggleLimiter),
        "push_to_mute" => Ok(ButtonAction::PushToMute),
        "push_to_talk" => Ok(ButtonAction::PushToTalk),
        "next_output" => Ok(ButtonAction::NextOutput),
        "prev_output" => Ok(ButtonAction::PrevOutput),
        "page_left" => Ok(ButtonAction::PageLeft),
        "page_right" => Ok(ButtonAction::PageRight),
        "none" => Ok(ButtonAction::None),
        s if s.starts_with("mute_output:") => {
            let id: u32 = s[12..].parse().map_err(|_| "invalid output id")?;
            Ok(ButtonAction::MuteOutput { output_id: id })
        }
        s if s.starts_with("load_profile:") => {
            Ok(ButtonAction::LoadProfile { name: s[13..].to_string() })
        }
        _ => Err(format!("unknown action: {s}")),
    }
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Get a config section (beacn, ui, applet, cli)
    Get { section: String },
    /// Set a config section (accepts JSON)
    Set { section: String, json: String },
}

#[derive(Subcommand)]
enum InputCmd {
    /// List all inputs
    List,
    /// Get an input by ID
    Get { id: u32 },
    /// Add a new input
    Add { name: String, color: String },
    /// Remove an input
    Remove { id: u32 },
    /// Move an input to a position in the list (0-indexed)
    Move { id: u32, position: u32 },
    /// Set an input's name
    SetName { id: u32, name: String },
    /// Set an input's color
    SetColor { id: u32, color: String },
    /// Get the default input
    GetDefault,
    /// Set the default input (0 to clear)
    SetDefault { id: u32 },
    /// Manage input EQ
    Eq {
        #[command(subcommand)]
        cmd: InputEqCmd,
    },
    /// Manage input noise gate
    Gate {
        #[command(subcommand)]
        cmd: InputGateCmd,
    },
    /// Manage input de-esser
    Deesser {
        #[command(subcommand)]
        cmd: InputDeesserCmd,
    },
}

#[derive(Subcommand)]
enum OutputCmd {
    /// List all outputs
    List,
    /// Get an output by ID
    Get { id: u32 },
    /// Add a new output (copies routes from source_output_id, use 0 for defaults)
    Add { name: String, color: String, source_output_id: u32 },
    /// Remove an output
    Remove { id: u32 },
    /// Move an output to a position in the list (0-indexed)
    Move { id: u32, position: u32 },
    /// Set an output's name
    SetName { id: u32, name: String },
    /// Set an output's color
    SetColor { id: u32, color: String },
    /// Set an output's volume (0-100)
    SetVolume { id: u32, volume: u8 },
    /// Set an output's mute state (true/false)
    SetMute { id: u32, muted: String },
    /// Set an output's target hardware device (empty to clear)
    SetTarget { id: u32, device_name: String },
    /// Get the default output
    GetDefault,
    /// Set the default output (0 to clear)
    SetDefault { id: u32 },
    /// Manage output compressor
    Compressor {
        #[command(subcommand)]
        cmd: OutputCompressorCmd,
    },
    /// Manage output limiter
    Limiter {
        #[command(subcommand)]
        cmd: OutputLimiterCmd,
    },
}

#[derive(Subcommand)]
enum InputEqCmd {
    /// Enable EQ for an input
    Enable { id: u32 },
    /// Disable EQ for an input
    Disable { id: u32 },
    /// Set an EQ band (band 0-7, band_type: peaking/low_shelf/high_shelf/bypass)
    Set {
        id: u32,
        band: u8,
        band_type: String,
        freq: f64,
        gain_db: f64,
        q: f64,
    },
    /// Get EQ settings for an input
    Get { id: u32 },
    /// Reset EQ to defaults for an input
    Reset { id: u32 },
}

#[derive(Subcommand)]
enum InputGateCmd {
    /// Enable gate for an input
    Enable { id: u32 },
    /// Disable gate for an input
    Disable { id: u32 },
    /// Set gate parameters
    Set {
        id: u32,
        threshold_db: f64,
        attack_ms: f64,
        release_ms: f64,
        hold_ms: f64,
    },
    /// Get gate settings for an input
    Get { id: u32 },
}

#[derive(Subcommand)]
enum InputDeesserCmd {
    /// Enable de-esser for an input
    Enable { id: u32 },
    /// Disable de-esser for an input
    Disable { id: u32 },
    /// Set de-esser parameters
    Set {
        id: u32,
        frequency: f64,
        threshold_db: f64,
        ratio: f64,
    },
    /// Get de-esser settings for an input
    Get { id: u32 },
}

#[derive(Subcommand)]
enum OutputCompressorCmd {
    /// Enable compressor for an output
    Enable { id: u32 },
    /// Disable compressor for an output
    Disable { id: u32 },
    /// Set compressor parameters
    Set {
        id: u32,
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
        makeup_gain_db: f64,
        knee_db: f64,
    },
    /// Get compressor settings for an output
    Get { id: u32 },
}

#[derive(Subcommand)]
enum OutputLimiterCmd {
    /// Enable limiter for an output
    Enable { id: u32 },
    /// Disable limiter for an output
    Disable { id: u32 },
    /// Set limiter parameters
    Set {
        id: u32,
        ceiling_db: f64,
        release_ms: f64,
    },
    /// Get limiter settings for an output
    Get { id: u32 },
}

#[derive(Subcommand)]
enum RouteCmd {
    /// Get a route (input→output)
    Get { input_id: u32, output_id: u32 },
    /// List all routes for an output
    List { output_id: u32 },
    /// Set a route's volume (0-100)
    SetVolume { input_id: u32, output_id: u32, volume: u8 },
    /// Set a route's mute state (true/false)
    SetMute { input_id: u32, output_id: u32, muted: String },
}

#[derive(Subcommand)]
enum StreamCmd {
    /// List all active audio streams
    List,
    /// Assign a stream to an input
    Assign {
        /// PipeWire node ID of the stream
        pw_node_id: u32,
        /// Input ID to route to
        input_id: u32,
        /// Remember this assignment as an app rule
        #[arg(long)]
        remember: bool,
    },
}

#[derive(Subcommand)]
enum RuleCmd {
    /// List all app rules
    List,
    /// Set an app rule (add or update)
    Set { app_name: String, input_id: u32 },
    /// Remove an app rule
    Remove { app_name: String },
}

#[derive(Subcommand)]
enum CaptureCmd {
    /// List available hardware capture devices
    List,
    /// Add a capture device as a mixer input
    Add {
        /// PipeWire node ID of the capture device
        pw_node_id: u32,
        /// Name for the new input
        name: String,
        /// Color for the new input (#RRGGBB)
        color: String,
    },
    /// Bind a capture device to an existing input
    Bind { input_id: u32, device_name: String },
    /// Remove a capture input binding
    Remove { input_id: u32 },
    /// Set capture volume for an input
    SetVolume { input_id: u32, volume: f32 },
    /// Set capture mute for an input
    SetMute { input_id: u32, #[arg(value_parser = parse_bool)] muted: bool },
}

#[derive(Subcommand)]
enum LevelCmd {
    /// Get whether level broadcasting is enabled
    Get,
    /// Enable or disable level broadcasting
    Set { #[arg(value_parser = parse_bool)] enabled: bool },
    /// Get current input levels (one-shot)
    Poll,
    /// Watch input levels in real-time
    Watch,
}

#[derive(Subcommand)]
enum ComponentCmd {
    /// List connected components
    List,
}

#[derive(Subcommand)]
enum AdapterCmd {
    /// List connected device adapters with their capabilities
    List,
}

#[derive(Subcommand)]
enum PlaybackCmd {
    /// List available playback devices
    List,
}

#[derive(Subcommand)]
enum CustomInputCmd {
    /// List all custom inputs
    List,
    /// Add a new custom input
    Add {
        name: String,
        color: String,
        /// Type: display_brightness, keyboard_brightness, dsp_parameter, command, http, dbus
        custom_type: String,
        /// Type-specific params as JSON (e.g., '{"command":"brightnessctl set {value}%"}')
        #[arg(long, default_value = "{}")]
        params: String,
    },
    /// Remove a custom input
    Remove { id: u32 },
    /// Get current value (0-100)
    Get { id: u32 },
    /// Set value (0-100)
    Set { id: u32, value: u8 },
}

#[derive(Subcommand)]
enum ListenCmd {
    /// Listen for all signals
    All,
    /// Listen for state changes (output volume/mute + route changes)
    State,
    /// Listen for config changes (inputs + outputs)
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let conn = Connection::session().await?;
    let proxy = match MixCtlProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not connect to mixctl daemon ({e})");
            eprintln!("       is mixctl-daemon running?");
            std::process::exit(1);
        }
    };
    // Verify daemon is responsive
    if proxy.ping().await.is_err() {
        eprintln!("error: mixctl daemon is not responding");
        eprintln!("       is mixctl-daemon running?");
        std::process::exit(1);
    }
    proxy.register_component("cli").await.ok();

    match args.cmd {
        Cmd::Ping => {
            let resp = proxy.ping().await?;
            println!("{resp}");
        }
        Cmd::Config { cmd } => match cmd {
            ConfigCmd::Get { section } => {
                let json = proxy.get_config_section(&section).await?;
                println!("{json}");
            }
            ConfigCmd::Set { section, json } => {
                proxy.set_config_section(&section, &json).await?;
                println!("ok");
            }
        },
        Cmd::Profile { cmd } => match cmd {
            ProfileCmd::List => {
                let profiles = proxy.list_profiles().await?;
                if profiles.is_empty() {
                    println!("(no profiles saved)");
                } else {
                    for name in profiles {
                        println!("{name}");
                    }
                }
            }
            ProfileCmd::Save { name } => {
                proxy.save_profile(&name).await?;
                println!("profile '{name}' saved");
            }
            ProfileCmd::Load { name } => {
                proxy.load_profile(&name).await?;
                println!("profile '{name}' loaded");
            }
            ProfileCmd::Delete { name } => {
                proxy.delete_profile(&name).await?;
                println!("profile '{name}' deleted");
            }
        },
        Cmd::Status => {
            let status = proxy.get_audio_status().await?;
            println!("audio: {status}");
            let default_input = proxy.get_default_input().await?;
            if default_input > 0 {
                println!("default input: {default_input}");
            } else {
                println!("default input: (none)");
            }
            let default_output = proxy.get_default_output().await?;
            if default_output > 0 {
                println!("default output: {default_output}");
            } else {
                println!("default output: (none)");
            }
            let components = proxy.list_components().await?;
            if components.is_empty() {
                println!("components: (none)");
            } else {
                println!("components:");
                for c in &components {
                    println!("  {} ({})", c.component_type, c.bus_name);
                }
            }
        }
        Cmd::Input { cmd } => match cmd {
            InputCmd::List => {
                let inputs = proxy.list_inputs().await?;
                for inp in inputs {
                    println!("[{}] {} ({})", inp.id, inp.name, inp.color);
                }
            }
            InputCmd::Get { id } => {
                let inp = proxy.get_input(id).await?;
                println!("id:    {}", inp.id);
                println!("name:  {}", inp.name);
                println!("color: {}", inp.color);
            }
            InputCmd::Add { name, color } => {
                let id = proxy.add_input(&name, &color).await?;
                println!("ok (id={})", id);
            }
            InputCmd::Remove { id } => {
                proxy.remove_input(id).await?;
                println!("ok");
            }
            InputCmd::Move { id, position } => {
                proxy.move_input(id, position).await?;
                println!("ok");
            }
            InputCmd::SetName { id, name } => {
                proxy.set_input_name(id, &name).await?;
                println!("ok");
            }
            InputCmd::SetColor { id, color } => {
                proxy.set_input_color(id, &color).await?;
                println!("ok");
            }
            InputCmd::GetDefault => {
                let id = proxy.get_default_input().await?;
                if id > 0 {
                    println!("{id}");
                } else {
                    println!("(none)");
                }
            }
            InputCmd::SetDefault { id } => {
                proxy.set_default_input(id).await?;
                println!("ok");
            }
            InputCmd::Eq { cmd } => match cmd {
                InputEqCmd::Enable { id } => {
                    proxy.set_input_eq_enabled(id, true).await?;
                    println!("ok");
                }
                InputEqCmd::Disable { id } => {
                    proxy.set_input_eq_enabled(id, false).await?;
                    println!("ok");
                }
                InputEqCmd::Set { id, band, band_type, freq, gain_db, q } => {
                    proxy.set_input_eq_band(id, band, &band_type, freq, gain_db, q).await?;
                    println!("ok");
                }
                InputEqCmd::Get { id } => {
                    let enabled = proxy.get_input_eq_enabled(id).await?;
                    let bands = proxy.get_input_eq(id).await?;
                    println!("enabled: {enabled}");
                    for (i, b) in bands.iter().enumerate() {
                        println!(
                            "  band {i}: type={} freq={:.1} gain={:.1}dB q={:.2}",
                            b.band_type, b.frequency, b.gain_db, b.q
                        );
                    }
                }
                InputEqCmd::Reset { id } => {
                    proxy.reset_input_eq(id).await?;
                    println!("ok");
                }
            },
            InputCmd::Gate { cmd } => match cmd {
                InputGateCmd::Enable { id } => {
                    proxy.set_input_gate_enabled(id, true).await?;
                    println!("ok");
                }
                InputGateCmd::Disable { id } => {
                    proxy.set_input_gate_enabled(id, false).await?;
                    println!("ok");
                }
                InputGateCmd::Set { id, threshold_db, attack_ms, release_ms, hold_ms } => {
                    proxy.set_input_gate(id, threshold_db, attack_ms, release_ms, hold_ms).await?;
                    println!("ok");
                }
                InputGateCmd::Get { id } => {
                    let gate = proxy.get_input_gate(id).await?;
                    println!("enabled:      {}", gate.enabled);
                    println!("threshold_db: {:.1}", gate.threshold_db);
                    println!("attack_ms:    {:.1}", gate.attack_ms);
                    println!("release_ms:   {:.1}", gate.release_ms);
                    println!("hold_ms:      {:.1}", gate.hold_ms);
                }
            },
            InputCmd::Deesser { cmd } => match cmd {
                InputDeesserCmd::Enable { id } => {
                    proxy.set_input_deesser_enabled(id, true).await?;
                    println!("ok");
                }
                InputDeesserCmd::Disable { id } => {
                    proxy.set_input_deesser_enabled(id, false).await?;
                    println!("ok");
                }
                InputDeesserCmd::Set { id, frequency, threshold_db, ratio } => {
                    proxy.set_input_deesser(id, frequency, threshold_db, ratio).await?;
                    println!("ok");
                }
                InputDeesserCmd::Get { id } => {
                    let ds = proxy.get_input_deesser(id).await?;
                    println!("enabled:      {}", ds.enabled);
                    println!("frequency:    {:.1}", ds.frequency);
                    println!("threshold_db: {:.1}", ds.threshold_db);
                    println!("ratio:        {:.1}", ds.ratio);
                }
            },
        },
        Cmd::Output { cmd } => match cmd {
            OutputCmd::List => {
                let outputs = proxy.list_outputs().await?;
                for out in outputs {
                    let mute_tag = if out.muted { " [MUTED]" } else { "" };
                    println!(
                        "[{}] {} ({}) vol={}{mute_tag}",
                        out.id, out.name, out.color, out.volume
                    );
                }
            }
            OutputCmd::Get { id } => {
                let out = proxy.get_output(id).await?;
                println!("id:     {}", out.id);
                println!("name:   {}", out.name);
                println!("color:  {}", out.color);
                println!("volume: {}", out.volume);
                println!("muted:  {}", out.muted);
            }
            OutputCmd::Add { name, color, source_output_id } => {
                let id = proxy.add_output(&name, &color, source_output_id).await?;
                println!("ok (id={})", id);
            }
            OutputCmd::Remove { id } => {
                proxy.remove_output(id).await?;
                println!("ok");
            }
            OutputCmd::Move { id, position } => {
                proxy.move_output(id, position).await?;
                println!("ok");
            }
            OutputCmd::SetName { id, name } => {
                proxy.set_output_name(id, &name).await?;
                println!("ok");
            }
            OutputCmd::SetColor { id, color } => {
                proxy.set_output_color(id, &color).await?;
                println!("ok");
            }
            OutputCmd::SetVolume { id, volume } => {
                proxy.set_output_volume(id, volume).await?;
                println!("ok");
            }
            OutputCmd::SetMute { id, muted } => {
                let muted = parse_bool(&muted)
                    .map_err(|e| anyhow::anyhow!(e))?;
                proxy.set_output_mute(id, muted).await?;
                println!("ok");
            }
            OutputCmd::SetTarget { id, device_name } => {
                proxy.set_output_target(id, &device_name).await?;
                println!("ok");
            }
            OutputCmd::GetDefault => {
                let id = proxy.get_default_output().await?;
                if id > 0 {
                    println!("{id}");
                } else {
                    println!("(none)");
                }
            }
            OutputCmd::SetDefault { id } => {
                proxy.set_default_output(id).await?;
                println!("ok");
            }
            OutputCmd::Compressor { cmd } => match cmd {
                OutputCompressorCmd::Enable { id } => {
                    proxy.set_output_compressor_enabled(id, true).await?;
                    println!("ok");
                }
                OutputCompressorCmd::Disable { id } => {
                    proxy.set_output_compressor_enabled(id, false).await?;
                    println!("ok");
                }
                OutputCompressorCmd::Set { id, threshold_db, ratio, attack_ms, release_ms, makeup_gain_db, knee_db } => {
                    proxy.set_output_compressor(id, threshold_db, ratio, attack_ms, release_ms, makeup_gain_db, knee_db).await?;
                    println!("ok");
                }
                OutputCompressorCmd::Get { id } => {
                    let comp = proxy.get_output_compressor(id).await?;
                    println!("enabled:        {}", comp.enabled);
                    println!("threshold_db:   {:.1}", comp.threshold_db);
                    println!("ratio:          {:.1}", comp.ratio);
                    println!("attack_ms:      {:.1}", comp.attack_ms);
                    println!("release_ms:     {:.1}", comp.release_ms);
                    println!("makeup_gain_db: {:.1}", comp.makeup_gain_db);
                    println!("knee_db:        {:.1}", comp.knee_db);
                }
            },
            OutputCmd::Limiter { cmd } => match cmd {
                OutputLimiterCmd::Enable { id } => {
                    proxy.set_output_limiter_enabled(id, true).await?;
                    println!("ok");
                }
                OutputLimiterCmd::Disable { id } => {
                    proxy.set_output_limiter_enabled(id, false).await?;
                    println!("ok");
                }
                OutputLimiterCmd::Set { id, ceiling_db, release_ms } => {
                    proxy.set_output_limiter(id, ceiling_db, release_ms).await?;
                    println!("ok");
                }
                OutputLimiterCmd::Get { id } => {
                    let lim = proxy.get_output_limiter(id).await?;
                    println!("enabled:    {}", lim.enabled);
                    println!("ceiling_db: {:.1}", lim.ceiling_db);
                    println!("release_ms: {:.1}", lim.release_ms);
                }
            },
        },
        Cmd::Route { cmd } => match cmd {
            RouteCmd::Get { input_id, output_id } => {
                let route = proxy.get_route(input_id, output_id).await?;
                println!("input_id:  {}", route.input_id);
                println!("output_id: {}", route.output_id);
                println!("volume:    {}", route.volume);
                println!("muted:     {}", route.muted);
            }
            RouteCmd::List { output_id } => {
                let routes = proxy.list_routes_for_output(output_id).await?;
                for r in routes {
                    let mute_tag = if r.muted { " [MUTED]" } else { "" };
                    println!(
                        "input={} → output={} vol={}{mute_tag}",
                        r.input_id, r.output_id, r.volume
                    );
                }
            }
            RouteCmd::SetVolume { input_id, output_id, volume } => {
                proxy.set_route_volume(input_id, output_id, volume).await?;
                println!("ok");
            }
            RouteCmd::SetMute { input_id, output_id, muted } => {
                let muted = parse_bool(&muted)
                    .map_err(|e| anyhow::anyhow!(e))?;
                proxy.set_route_mute(input_id, output_id, muted).await?;
                println!("ok");
            }
        },
        Cmd::Stream { cmd } => match cmd {
            StreamCmd::List => {
                let streams = proxy.list_streams().await?;
                if streams.is_empty() {
                    println!("(no active streams)");
                } else {
                    for s in streams {
                        let input_tag = if s.input_id > 0 {
                            format!("→ input {}", s.input_id)
                        } else {
                            "unassigned".to_string()
                        };
                        println!(
                            "[{}] {} - {} ({input_tag})",
                            s.pw_node_id, s.app_name, s.media_name
                        );
                    }
                }
            }
            StreamCmd::Assign {
                pw_node_id,
                input_id,
                remember,
            } => {
                proxy.assign_stream(pw_node_id, input_id, remember).await?;
                println!("ok");
            }
        },
        Cmd::Rule { cmd } => match cmd {
            RuleCmd::List => {
                let rules = proxy.list_app_rules().await?;
                if rules.is_empty() {
                    println!("(no rules)");
                } else {
                    for r in rules {
                        println!("{} → input {}", r.app_name, r.input_id);
                    }
                }
            }
            RuleCmd::Set { app_name, input_id } => {
                proxy.set_app_rule(&app_name, input_id).await?;
                println!("ok");
            }
            RuleCmd::Remove { app_name } => {
                proxy.remove_app_rule(&app_name).await?;
                println!("ok");
            }
        },
        Cmd::Capture { cmd } => match cmd {
            CaptureCmd::List => {
                let devices = proxy.list_capture_devices().await?;
                if devices.is_empty() {
                    println!("(no capture devices)");
                } else {
                    for d in devices {
                        let status = if d.is_added {
                            format!("added as input {}", d.input_id)
                        } else {
                            "available".to_string()
                        };
                        println!(
                            "[{}] {} ({}) - {status}",
                            d.pw_node_id, d.name, d.device_name
                        );
                    }
                }
            }
            CaptureCmd::Add {
                pw_node_id,
                name,
                color,
            } => {
                let id = proxy.add_capture_input(pw_node_id, &name, &color).await?;
                println!("ok (id={})", id);
            }
            CaptureCmd::Bind { input_id, device_name } => {
                proxy.bind_capture_to_input(input_id, &device_name).await?;
                println!("ok");
            }
            CaptureCmd::Remove { input_id } => {
                proxy.remove_capture_input(input_id).await?;
                println!("ok");
            }
            CaptureCmd::SetVolume { input_id, volume } => {
                proxy.set_capture_volume(input_id, volume).await?;
                println!("ok");
            }
            CaptureCmd::SetMute { input_id, muted } => {
                proxy.set_capture_mute(input_id, muted).await?;
                println!("ok");
            }
        },
        Cmd::Level { cmd } => match cmd {
            LevelCmd::Get => {
                let enabled = proxy.get_broadcast_levels().await?;
                println!("{enabled}");
            }
            LevelCmd::Set { enabled } => {
                proxy.set_broadcast_levels(enabled).await?;
                println!("ok");
            }
            LevelCmd::Poll => {
                let levels = proxy.get_input_levels().await?;
                if levels.is_empty() {
                    println!("(no levels — is broadcasting enabled?)");
                } else {
                    for (id, level) in levels {
                        println!("input {id}: {level:.4}");
                    }
                }
            }
            LevelCmd::Watch => {
                use futures_lite::StreamExt;
                proxy.set_broadcast_levels(true).await?;
                let mut stream = proxy.receive_input_levels_changed().await?;
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let parts: Vec<String> = args.levels.iter()
                            .map(|(id, lvl)| format!("{id}:{lvl:.3}"))
                            .collect();
                        println!("{}", parts.join(" "));
                    }
                }
            }
        },
        Cmd::Component { cmd } => match cmd {
            ComponentCmd::List => {
                let components = proxy.list_components().await?;
                if components.is_empty() {
                    println!("(no components connected)");
                } else {
                    for c in components {
                        println!("{} ({})", c.component_type, c.bus_name);
                    }
                }
            }
        },
        Cmd::Adapter { cmd } => match cmd {
            AdapterCmd::List => {
                let devices = proxy.list_devices().await?;
                if devices.is_empty() {
                    println!("(no device adapters connected)");
                } else {
                    for d in devices {
                        // Parse capabilities for a readable summary
                        let caps_summary = summarize_capabilities(&d.capabilities_json);
                        println!("{:<18} {}", d.device_name, caps_summary);
                    }
                }
            }
        },
        Cmd::Playback { cmd } => match cmd {
            PlaybackCmd::List => {
                let devices = proxy.list_playback_devices().await?;
                if devices.is_empty() {
                    println!("(no playback devices)");
                } else {
                    for d in devices {
                        println!("[{}] {} ({})", d.pw_node_id, d.name, d.device_name);
                    }
                }
            }
        },
        Cmd::Beacn { cmd } => {
            let json = proxy.get_config_section("beacn").await?;
            let mut config: BeacnConfig = serde_json::from_str(&json)
                .map_err(|e| anyhow::anyhow!("failed to parse beacn config: {e}"))?;
            match cmd {
                BeacnCmd::Status => {
                    println!("layout:             {}", config.layout);
                    println!("dial_sensitivity:   {}", config.dial_sensitivity);
                    println!("level_decay:        {}", config.level_decay);
                    println!("display_brightness: {}", config.display_brightness);
                    println!("led_brightness:     {}", config.led_brightness);
                    println!("hold_threshold_ms:  {}", config.hold_threshold_ms);
                    println!();
                    println!("button mappings:");
                    for name in ButtonMappings::BUTTON_NAMES {
                        let mapping = config.button_mappings.get(name).unwrap();
                        println!(
                            "  {:<12} press={:<20} hold={}",
                            format!("{name}:"),
                            mapping.press.display_name(),
                            mapping.hold.display_name()
                        );
                    }
                }
                BeacnCmd::Buttons => {
                    for name in ButtonMappings::BUTTON_NAMES {
                        let mapping = config.button_mappings.get(name).unwrap();
                        println!(
                            "{:<12} press={:<20} hold={}",
                            format!("{name}:"),
                            mapping.press.display_name(),
                            mapping.hold.display_name()
                        );
                    }
                }
                BeacnCmd::Button { name, trigger, action } => {
                    let parsed_action = parse_button_action(&action)
                        .map_err(|e| anyhow::anyhow!(e))?;
                    let current = config.button_mappings.get(&name)
                        .ok_or_else(|| anyhow::anyhow!("unknown button: {name}"))?
                        .clone();
                    let new_mapping = match trigger.as_str() {
                        "press" => ButtonMapping { press: parsed_action, hold: current.hold },
                        "hold" => ButtonMapping { press: current.press, hold: parsed_action },
                        _ => return Err(anyhow::anyhow!("trigger must be 'press' or 'hold', got '{trigger}'")),
                    };
                    if !config.button_mappings.set(&name, new_mapping) {
                        return Err(anyhow::anyhow!("unknown button: {name}"));
                    }
                    let json = serde_json::to_string(&config)?;
                    proxy.set_config_section("beacn", &json).await?;
                    println!("ok");
                }
                BeacnCmd::Set { key, value } => {
                    match key.as_str() {
                        "layout" => {
                            config.layout = value;
                        }
                        "dial-sensitivity" => {
                            config.dial_sensitivity = value.parse()
                                .map_err(|_| anyhow::anyhow!("invalid dial-sensitivity: {value}"))?;
                        }
                        "hold-threshold" => {
                            config.hold_threshold_ms = value.parse()
                                .map_err(|_| anyhow::anyhow!("invalid hold-threshold: {value}"))?;
                        }
                        "display-brightness" => {
                            config.display_brightness = value.parse()
                                .map_err(|_| anyhow::anyhow!("invalid display-brightness (0-255): {value}"))?;
                        }
                        "led-brightness" => {
                            config.led_brightness = value.parse()
                                .map_err(|_| anyhow::anyhow!("invalid led-brightness (0-255): {value}"))?;
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "unknown key: {key}\nvalid keys: layout, dial-sensitivity, hold-threshold, display-brightness, led-brightness"
                            ));
                        }
                    }
                    let json = serde_json::to_string(&config)?;
                    proxy.set_config_section("beacn", &json).await?;
                    println!("ok");
                }
            }
        }
        Cmd::CustomInput { cmd } => match cmd {
            CustomInputCmd::List => {
                let inputs = proxy.list_custom_inputs().await?;
                if inputs.is_empty() {
                    println!("(no custom inputs)");
                } else {
                    let id_w = inputs.iter().map(|i| i.id.to_string().len()).max().unwrap_or(1);
                    let name_w = inputs.iter().map(|i| i.name.len()).max().unwrap_or(1);
                    let type_w = inputs.iter().map(|i| i.custom_type.len()).max().unwrap_or(1);
                    for ci in inputs {
                        println!(
                            "{:>id_w$}  {:<name_w$}  {:<type_w$}  {}",
                            ci.id, ci.name, ci.custom_type, ci.value,
                            id_w = id_w, name_w = name_w, type_w = type_w,
                        );
                    }
                }
            }
            CustomInputCmd::Add { name, color, custom_type, params } => {
                let id = proxy.add_custom_input(&name, &color, &custom_type, &params).await?;
                println!("ok (id={})", id);
            }
            CustomInputCmd::Remove { id } => {
                proxy.remove_custom_input(id).await?;
                println!("ok");
            }
            CustomInputCmd::Get { id } => {
                let value = proxy.get_custom_input_value(id).await?;
                println!("{value}");
            }
            CustomInputCmd::Set { id, value } => {
                proxy.set_custom_input_value(id, value).await?;
                println!("ok");
            }
        },
        Cmd::Listen { cmd } => {
            use futures_lite::StreamExt;

            match cmd {
                ListenCmd::All => {
                    let mut output_state_stream = proxy.receive_output_state_changed().await?;
                    let mut route_stream = proxy.receive_route_changed().await?;
                    let mut inputs_config_stream = proxy.receive_inputs_config_changed().await?;
                    let mut outputs_config_stream = proxy.receive_outputs_config_changed().await?;
                    let mut streams_stream = proxy.receive_streams_changed().await?;
                    let mut rules_stream = proxy.receive_app_rules_changed().await?;
                    let mut capture_stream = proxy.receive_capture_devices_changed().await?;
                    let mut audio_stream = proxy.receive_audio_status_changed().await?;
                    let mut playback_stream = proxy.receive_playback_devices_changed().await?;
                    let mut component_stream = proxy.receive_component_changed().await?;
                    let mut input_dsp_stream = proxy.receive_input_dsp_changed().await?;
                    let mut output_dsp_stream = proxy.receive_output_dsp_changed().await?;
                    let mut profile_stream = proxy.receive_profile_changed().await?;
                    loop {
                        tokio::select! {
                            Some(signal) = output_state_stream.next() => {
                                let id = signal.args().unwrap().id;
                                print_output_state_signal(&proxy, id).await;
                            }
                            Some(signal) = route_stream.next() => {
                                let args = signal.args().unwrap();
                                print_route_signal(&proxy, args.input_id, args.output_id).await;
                            }
                            Some(_) = inputs_config_stream.next() => {
                                print_inputs_config_signal(&proxy).await;
                            }
                            Some(_) = outputs_config_stream.next() => {
                                print_outputs_config_signal(&proxy).await;
                            }
                            Some(_) = streams_stream.next() => {
                                println!("streams_changed");
                            }
                            Some(_) = rules_stream.next() => {
                                println!("app_rules_changed");
                            }
                            Some(_) = capture_stream.next() => {
                                println!("capture_devices_changed");
                            }
                            Some(_) = audio_stream.next() => {
                                match proxy.get_audio_status().await {
                                    Ok(status) => println!("audio_status_changed: {status}"),
                                    Err(e) => println!("audio_status_changed: (fetch failed: {e})"),
                                }
                            }
                            Some(_) = playback_stream.next() => {
                                println!("playback_devices_changed");
                            }
                            Some(_) = component_stream.next() => {
                                println!("component_changed");
                            }
                            Some(signal) = input_dsp_stream.next() => {
                                let id = signal.args().unwrap().input_id;
                                println!("input_dsp_changed: input={id}");
                            }
                            Some(signal) = output_dsp_stream.next() => {
                                let id = signal.args().unwrap().output_id;
                                println!("output_dsp_changed: output={id}");
                            }
                            Some(signal) = profile_stream.next() => {
                                let name = signal.args().unwrap().name.clone();
                                println!("profile_changed: {name}");
                            }
                        }
                    }
                }
                ListenCmd::State => {
                    let mut output_state_stream = proxy.receive_output_state_changed().await?;
                    let mut route_stream = proxy.receive_route_changed().await?;
                    loop {
                        futures_lite::future::or(
                            async {
                                if let Some(signal) = output_state_stream.next().await {
                                    let id = signal.args().unwrap().id;
                                    print_output_state_signal(&proxy, id).await;
                                }
                            },
                            async {
                                if let Some(signal) = route_stream.next().await {
                                    let args = signal.args().unwrap();
                                    print_route_signal(&proxy, args.input_id, args.output_id).await;
                                }
                            },
                        )
                        .await;
                    }
                }
                ListenCmd::Config => {
                    let mut inputs_stream = proxy.receive_inputs_config_changed().await?;
                    let mut outputs_stream = proxy.receive_outputs_config_changed().await?;
                    loop {
                        futures_lite::future::or(
                            async {
                                if let Some(_) = inputs_stream.next().await {
                                    print_inputs_config_signal(&proxy).await;
                                }
                            },
                            async {
                                if let Some(_) = outputs_stream.next().await {
                                    print_outputs_config_signal(&proxy).await;
                                }
                            },
                        )
                        .await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn print_output_state_signal(proxy: &MixCtlProxy<'_>, id: u32) {
    match proxy.get_output(id).await {
        Ok(out) => {
            let mute_tag = if out.muted { " [MUTED]" } else { "" };
            println!(
                "output_state_changed: [{}] {} vol={}{mute_tag}",
                out.id, out.name, out.volume
            );
        }
        Err(e) => println!("output_state_changed: id={id} (fetch failed: {e})"),
    }
}

async fn print_route_signal(proxy: &MixCtlProxy<'_>, input_id: u32, output_id: u32) {
    match proxy.get_route(input_id, output_id).await {
        Ok(r) => {
            let mute_tag = if r.muted { " [MUTED]" } else { "" };
            println!(
                "route_changed: input={} → output={} vol={}{mute_tag}",
                r.input_id, r.output_id, r.volume
            );
        }
        Err(e) => println!("route_changed: input={input_id} output={output_id} (fetch failed: {e})"),
    }
}

async fn print_inputs_config_signal(proxy: &MixCtlProxy<'_>) {
    match proxy.list_inputs().await {
        Ok(inputs) => {
            println!("inputs_config_changed: {} inputs", inputs.len());
            for inp in inputs {
                println!("  [{}] {} ({})", inp.id, inp.name, inp.color);
            }
        }
        Err(e) => println!("inputs_config_changed: (fetch failed: {e})"),
    }
}

async fn print_outputs_config_signal(proxy: &MixCtlProxy<'_>) {
    match proxy.list_outputs().await {
        Ok(outputs) => {
            println!("outputs_config_changed: {} outputs", outputs.len());
            for out in outputs {
                let mute_tag = if out.muted { " [MUTED]" } else { "" };
                println!(
                    "  [{}] {} ({}) vol={}{mute_tag}",
                    out.id, out.name, out.color, out.volume
                );
            }
        }
        Err(e) => println!("outputs_config_changed: (fetch failed: {e})"),
    }
}
