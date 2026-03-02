mod config;
mod dbus_adapter;
mod service;
mod state;

use std::time::Duration;

use mixctl_core::dbus::{BUS_NAME, OBJ_PATH};
use tracing::{info, warn, Level};
use tracing_subscriber::EnvFilter;
use zbus::connection::Builder as ConnectionBuilder;

use crate::config::ConfigFile;
use crate::service::Service;
use crate::state::StateFile;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    // Load config (creates with defaults if missing)
    let config = ConfigFile::load_or_create()?;
    info!("loaded config with {} channels", config.channels.len());

    // Load state and reconcile with config
    let mut state = StateFile::load()?;
    let reconciled = state.reconcile(&config);

    let svc = Service::new(config, state);

    // Mark state dirty if reconcile made changes
    if reconciled {
        svc.inner.lock().await.state_dirty = true;
    }

    let _conn = ConnectionBuilder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJ_PATH, svc.clone())?
        .build()
        .await?;

    info!("daemon running: {} {}", BUS_NAME, OBJ_PATH);

    // Periodic flush task (30s)
    let flush_svc = svc.clone();
    let flush_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut shared = flush_svc.inner.lock().await;
            if shared.config_dirty {
                if let Err(e) = shared.config.save() {
                    warn!("failed to flush config: {e}");
                } else {
                    shared.config_dirty = false;
                }
            }
            if shared.state_dirty {
                if let Err(e) = shared.state.save() {
                    warn!("failed to flush state: {e}");
                } else {
                    shared.state_dirty = false;
                }
            }
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("shutting down");

    // Final flush
    flush_handle.abort();
    let shared = svc.inner.lock().await;
    if shared.config_dirty {
        shared.config.save()?;
    }
    if shared.state_dirty {
        shared.state.save()?;
    }

    Ok(())
}
