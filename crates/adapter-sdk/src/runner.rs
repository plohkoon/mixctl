use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_lite::StreamExt;
use mixctl_core::dbus::MixCtlProxy;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::adapter::DeviceAdapter;
use crate::types::MixerEvent;

/// Manages the D-Bus lifecycle for a device adapter.
///
/// Handles connection, reconnection, signal subscription, capability
/// registration, and panic recovery. The adapter only needs to implement
/// `DeviceAdapter` and provide its hardware I/O logic.
///
/// ```text
///   ┌──────────────────────────────────────────────────┐
///   │  AdapterRunner                                   │
///   │  ┌────────────┐  ┌───────────────────────────┐   │
///   │  │ D-Bus conn │  │ Signal subscriptions      │   │
///   │  │ + reconnect│  │ → MixerEvent channel      │   │
///   │  └─────┬──────┘  └────────────┬──────────────┘   │
///   │        │                      │                  │
///   │        ▼                      ▼                  │
///   │  ┌─────────────────────────────────────────┐     │
///   │  │ adapter.run(proxy, mixer_events)         │     │
///   │  │ (adapter owns its hardware I/O loop)     │     │
///   │  └─────────────────────────────────────────┘     │
///   └──────────────────────────────────────────────────┘
/// ```
pub struct AdapterRunner {
    shutdown_flag: Arc<AtomicBool>,
}

impl AdapterRunner {
    pub fn new() -> Self {
        Self {
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns a handle to the shutdown flag. Set to `true` to request shutdown.
    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        self.shutdown_flag.clone()
    }

    /// Run the adapter with automatic D-Bus reconnection.
    ///
    /// This is the main entry point. It loops, connecting to D-Bus and running
    /// the adapter. If the D-Bus connection drops, it waits and retries.
    /// The loop exits when the shutdown flag is set or the adapter returns Ok(()).
    pub async fn run<A: DeviceAdapter>(&self, adapter: &mut A) -> anyhow::Result<()> {
        info!(
            device = adapter.device_name(),
            "adapter starting"
        );

        loop {
            if self.shutdown_flag.load(Ordering::Acquire) {
                break;
            }

            match self.run_session(adapter).await {
                Ok(()) => {
                    info!("adapter session ended cleanly");
                    break;
                }
                Err(e) => {
                    warn!("adapter session ended: {e}");
                    // Wait before retrying, checking shutdown flag periodically
                    for _ in 0..20 {
                        if self.shutdown_flag.load(Ordering::Acquire) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }

        adapter.shutdown().await;
        info!("adapter stopped");
        Ok(())
    }

    /// Run a single D-Bus session: connect, register, subscribe, run adapter.
    async fn run_session<A: DeviceAdapter>(&self, adapter: &mut A) -> anyhow::Result<()> {
        // Connect to D-Bus
        let conn = zbus::Connection::session().await?;
        let proxy = MixCtlProxy::new(&conn).await?;

        // Verify daemon is alive
        proxy.ping().await?;
        info!("connected to mixer daemon via D-Bus");

        // Register device with capabilities
        let caps_json = serde_json::to_string(&adapter.capabilities())?;
        proxy
            .register_device(adapter.device_name(), &caps_json)
            .await
            .map_err(|e| anyhow::anyhow!("failed to register device: {e}"))?;
        info!(
            device = adapter.device_name(),
            capabilities = caps_json,
            "registered device"
        );

        // Subscribe to all D-Bus signals, routing to a channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let signal_tasks = self.subscribe_signals(&proxy, event_tx).await?;

        // Run the adapter's main loop.
        // Panic recovery is handled at the process level: systemd Restart=on-failure
        // restarts the adapter if it panics. This is more reliable than catch_unwind
        // for async code, and avoids the complexity of poisoned state after unwind.
        let result = adapter.run(proxy, event_rx).await;

        // Cleanup signal tasks
        for task in signal_tasks.into_iter() {
            task.abort();
        }

        result
    }

    /// Subscribe to all mixer daemon D-Bus signals and route them as MixerEvents.
    async fn subscribe_signals(
        &self,
        proxy: &MixCtlProxy<'_>,
        tx: mpsc::UnboundedSender<MixerEvent>,
    ) -> anyhow::Result<Vec<JoinHandle<()>>> {
        let mut tasks = Vec::new();

        // Helper: spawn a task that reads from a signal stream and sends events
        macro_rules! signal_task {
            ($stream:expr, $event:expr) => {{
                let tx = tx.clone();
                let mut stream = $stream;
                tasks.push(tokio::spawn(async move {
                    while stream.next().await.is_some() {
                        if tx.send($event).is_err() {
                            break;
                        }
                    }
                }));
            }};
        }

        // Simple signals (no payload)
        signal_task!(
            proxy.receive_inputs_config_changed().await?,
            MixerEvent::InputsChanged
        );
        signal_task!(
            proxy.receive_outputs_config_changed().await?,
            MixerEvent::OutputsChanged
        );
        signal_task!(
            proxy.receive_streams_changed().await?,
            MixerEvent::StreamsChanged
        );
        signal_task!(
            proxy.receive_audio_status_changed().await?,
            MixerEvent::AudioStatusChanged
        );
        signal_task!(
            proxy.receive_app_rules_changed().await?,
            MixerEvent::InputsChanged // app rules affect input routing
        );
        signal_task!(
            proxy.receive_capture_devices_changed().await?,
            MixerEvent::CaptureDevicesChanged
        );
        signal_task!(
            proxy.receive_playback_devices_changed().await?,
            MixerEvent::PlaybackDevicesChanged
        );
        signal_task!(
            proxy.receive_component_changed().await?,
            MixerEvent::ComponentChanged
        );

        // Signals with payloads need individual handlers
        {
            let tx = tx.clone();
            let mut stream = proxy.receive_output_state_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::OutputStateChanged { id: args.id });
                    }
                }
            }));
        }

        {
            let tx = tx.clone();
            let mut stream = proxy.receive_route_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::RouteChanged {
                            input_id: args.input_id,
                            output_id: args.output_id,
                        });
                    }
                }
            }));
        }

        {
            let tx = tx.clone();
            let mut stream = proxy.receive_input_levels_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::LevelsChanged {
                            levels: args.levels.clone(),
                        });
                    }
                }
            }));
        }

        {
            let tx = tx.clone();
            let mut stream = proxy.receive_broadcast_levels_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::BroadcastLevelsChanged {
                            enabled: args.enabled,
                        });
                    }
                }
            }));
        }

        {
            let tx = tx.clone();
            let mut stream = proxy.receive_config_section_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::ConfigSectionChanged {
                            section: args.section.clone(),
                        });
                    }
                }
            }));
        }

        {
            let tx = tx.clone();
            let mut stream = proxy.receive_custom_input_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::CustomInputChanged { id: args.id });
                    }
                }
            }));
        }

        {
            let tx = tx.clone();
            let mut stream = proxy.receive_input_dsp_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::InputDspChanged {
                            input_id: args.input_id,
                        });
                    }
                }
            }));
        }

        {
            let tx = tx.clone();
            let mut stream = proxy.receive_output_dsp_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::OutputDspChanged {
                            output_id: args.output_id,
                        });
                    }
                }
            }));
        }

        {
            let tx = tx.clone();
            let mut stream = proxy.receive_profile_changed().await?;
            tasks.push(tokio::spawn(async move {
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        let _ = tx.send(MixerEvent::ProfileChanged {
                            name: args.name.clone(),
                        });
                    }
                }
            }));
        }

        Ok(tasks)
    }
}

impl Default for AdapterRunner {
    fn default() -> Self {
        Self::new()
    }
}
