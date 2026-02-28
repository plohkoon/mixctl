mod dbus_adapter; // just needs to be included so the macro impl is compiled
mod service;

use mixctl_core::dbus::{BUS_NAME, OBJ_PATH}; // your constants
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;
use zbus::ConnectionBuilder;

use crate::service::Service;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    let svc = Service::new();

    let _conn = ConnectionBuilder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJ_PATH, svc)?
        .build()
        .await?;

    info!("daemon running: {} {}", BUS_NAME, OBJ_PATH);

    tokio::signal::ctrl_c().await?;
    Ok(())
}
