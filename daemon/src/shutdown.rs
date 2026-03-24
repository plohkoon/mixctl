use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{info, warn};

use crate::audio::{PwCommand, PwEngine};
use crate::service::Service;

pub struct ShutdownGuard {
    svc: Service,
    shutdown_flag: Arc<AtomicBool>,
    pw_chan_tx: Arc<tokio::sync::Mutex<Option<pipewire::channel::Sender<PwCommand>>>>,
    engine: Option<PwEngine>,
    handles: Vec<tokio::task::JoinHandle<()>>,
}

impl ShutdownGuard {
    pub fn new(
        svc: Service,
        shutdown_flag: Arc<AtomicBool>,
        pw_chan_tx: Arc<tokio::sync::Mutex<Option<pipewire::channel::Sender<PwCommand>>>>,
        engine: PwEngine,
        handles: Vec<tokio::task::JoinHandle<()>>,
    ) -> Self {
        Self {
            svc,
            shutdown_flag,
            pw_chan_tx,
            engine: Some(engine),
            handles,
        }
    }
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        info!("shutdown guard: cleaning up");

        // 1. Persist stream assignments + build Shutdown command from Shared
        let cmd = if let Ok(mut shared) = self.svc.inner.try_lock() {
            shared.persist_stream_assignments();
            PwCommand::Shutdown {
                original_default_sink: shared.original_default_sink.clone(),
                original_default_source: shared.original_default_source.clone(),
                original_stream_targets: shared.original_stream_targets.clone(),
            }
        } else {
            warn!("shutdown guard: couldn't lock service, sending bare shutdown");
            PwCommand::Shutdown {
                original_default_sink: None,
                original_default_source: None,
                original_stream_targets: HashMap::new(),
            }
        };

        // 2. Signal PW reconnection loop to stop
        self.shutdown_flag.store(true, Ordering::Release);

        // 3. Send Shutdown directly to PW channel (bypass relay)
        if let Ok(guard) = self.pw_chan_tx.try_lock() {
            if let Some(tx) = guard.as_ref() {
                tx.send(cmd).ok();
            }
        }

        // 4. Abort all tasks
        for handle in self.handles.drain(..) {
            handle.abort();
        }

        // 5. Wait for PW thread
        if let Some(engine) = self.engine.take() {
            engine.join();
        }

        // 6. Final flush
        if let Ok(shared) = self.svc.inner.try_lock() {
            if shared.config_dirty {
                shared.config.save().ok();
            }
            if shared.state_dirty {
                shared.state.save().ok();
            }
        }
    }
}

/// Wait for SIGINT or SIGTERM.
pub async fn wait_for_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
    };
}
