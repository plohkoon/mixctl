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
    Ping,
    Status,
    SetProfile { name: String },
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
        Cmd::Status => {
            let json = proxy.get_state_json().await?;
            println!("{json}");
        }
        Cmd::SetProfile { name } => {
            proxy.set_profile(&name).await?;
            println!("ok");
        }
    }

    Ok(())
}
