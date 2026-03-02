use anyhow::Result;
use clap::{Parser, Subcommand};
use mixctl_core::dbus::MixCtlProxy;
use zbus::Connection;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ping the daemon
    Ping,
    /// Channel management
    Channel {
        #[command(subcommand)]
        cmd: ChannelCmd,
    },
    /// Page management
    Page {
        #[command(subcommand)]
        cmd: PageCmd,
    },
}

#[derive(Subcommand)]
enum ChannelCmd {
    /// List all channels
    List,
    /// Get a channel by ID
    Get { id: u32 },
    /// Add a new channel
    Add { id: u32, name: String, color: String },
    /// Remove a channel
    Remove { id: u32 },
    /// Set a channel's name
    SetName { id: u32, name: String },
    /// Set a channel's color
    SetColor { id: u32, color: String },
    /// Set a channel's mute state
    SetMute { id: u32, muted: bool },
    /// Set a channel's volume (0-100)
    SetVolume { id: u32, volume: u8 },
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
        Cmd::Channel { cmd } => match cmd {
            ChannelCmd::List => {
                let channels = proxy.list_channels().await?;
                for ch in channels {
                    let mute_tag = if ch.muted { " [MUTED]" } else { "" };
                    println!(
                        "[{}] {} ({}) vol={}{mute_tag}",
                        ch.id, ch.name, ch.color, ch.volume
                    );
                }
            }
            ChannelCmd::Get { id } => {
                let ch = proxy.get_channel(id).await?;
                println!("id:     {}", ch.id);
                println!("name:   {}", ch.name);
                println!("color:  {}", ch.color);
                println!("muted:  {}", ch.muted);
                println!("volume: {}", ch.volume);
            }
            ChannelCmd::Add { id, name, color } => {
                proxy.add_channel(id, &name, &color).await?;
                println!("ok");
            }
            ChannelCmd::Remove { id } => {
                proxy.remove_channel(id).await?;
                println!("ok");
            }
            ChannelCmd::SetName { id, name } => {
                proxy.set_channel_name(id, &name).await?;
                println!("ok");
            }
            ChannelCmd::SetColor { id, color } => {
                proxy.set_channel_color(id, &color).await?;
                println!("ok");
            }
            ChannelCmd::SetMute { id, muted } => {
                proxy.set_channel_mute(id, muted).await?;
                println!("ok");
            }
            ChannelCmd::SetVolume { id, volume } => {
                proxy.set_channel_volume(id, volume).await?;
                println!("ok");
            }
        },
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
