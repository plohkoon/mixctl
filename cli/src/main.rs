use anyhow::Result;
use clap::{Parser, Subcommand};
use mixctl_core::dbus::MixCtlProxy;
use zbus::Connection;

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!("expected true/false, got '{s}'")),
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
    /// Page management
    Page {
        #[command(subcommand)]
        cmd: PageCmd,
    },
    /// Listen for daemon signals (runs until interrupted)
    Listen {
        #[command(subcommand)]
        cmd: ListenCmd,
    },
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
enum ListenCmd {
    /// Listen for all signals
    All,
    /// Listen for state changes (output volume/mute + route changes)
    State,
    /// Listen for config changes (inputs + outputs)
    Config,
    /// Listen for page changes
    Page,
}

#[derive(Subcommand)]
enum PageCmd {
    /// Get the current page
    Get,
    /// Set the current page
    Set { page: u32 },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let conn = Connection::session().await?;
    let proxy = MixCtlProxy::new(&conn).await?;

    match args.cmd {
        Cmd::Ping => {
            let resp = proxy.ping().await?;
            println!("{resp}");
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
        Cmd::Listen { cmd } => {
            use futures_lite::StreamExt;

            match cmd {
                ListenCmd::All => {
                    let mut output_state_stream = proxy.receive_output_state_changed().await?;
                    let mut route_stream = proxy.receive_route_changed().await?;
                    let mut inputs_config_stream = proxy.receive_inputs_config_changed().await?;
                    let mut outputs_config_stream = proxy.receive_outputs_config_changed().await?;
                    let mut page_stream = proxy.receive_page_changed().await?;
                    loop {
                        futures_lite::future::or(
                            futures_lite::future::or(
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
                                ),
                                futures_lite::future::or(
                                    async {
                                        if let Some(_) = inputs_config_stream.next().await {
                                            print_inputs_config_signal(&proxy).await;
                                        }
                                    },
                                    async {
                                        if let Some(_) = outputs_config_stream.next().await {
                                            print_outputs_config_signal(&proxy).await;
                                        }
                                    },
                                ),
                            ),
                            async {
                                if let Some(signal) = page_stream.next().await {
                                    let args = signal.args().unwrap();
                                    println!("page_changed: {}", args.page);
                                }
                            },
                        )
                        .await;
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
                ListenCmd::Page => {
                    let mut stream = proxy.receive_page_changed().await?;
                    while let Some(signal) = stream.next().await {
                        let args = signal.args().unwrap();
                        println!("page_changed: {}", args.page);
                    }
                }
            }
        }
        Cmd::Page { cmd } => match cmd {
            PageCmd::Get => {
                let page = proxy.get_current_page().await?;
                println!("{page}");
            }
            PageCmd::Set { page } => {
                proxy.set_current_page(page).await?;
                println!("ok");
            }
        },
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
