use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mixctl_adapter_sdk::{
    AdapterRunner, ButtonKind, Capability, DeviceAdapter, MixCtlProxy, MixerEvent,
};
use tokio::sync::mpsc;
use tracing::{info, warn, Level};
use tracing_subscriber::EnvFilter;

struct MyAdapter {
    // TODO: Add your device I/O channels and state here
}

impl DeviceAdapter for MyAdapter {
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Fader {
                count: {{fader_count}},
                range: (0.0, 1.0),
            },
            Capability::Button {
                count: {{button_count}},
                kind: ButtonKind::Momentary,
            },
            // TODO: Add Screen, Led, Meter capabilities if your device has them
        ]
    }

    fn device_name(&self) -> &str {
        "{{device_name}}"
    }

    async fn run(
        &mut self,
        proxy: MixCtlProxy<'static>,
        mut mixer_events: mpsc::UnboundedReceiver<MixerEvent>,
    ) -> anyhow::Result<()> {
        info!("{{device_name}} adapter running");

        // TODO: Fetch initial mixer state
        // let inputs = proxy.list_inputs().await?;
        // let outputs = proxy.list_outputs().await?;

        loop {
            tokio::select! {
                // Handle mixer state changes from the SDK
                event = mixer_events.recv() => {
                    let Some(event) = event else { break };
                    match event {
                        MixerEvent::InputsChanged
                        | MixerEvent::OutputsChanged
                        | MixerEvent::RouteChanged { .. } => {
                            // TODO: Refresh your display / state
                            info!("mixer state changed");
                        }
                        MixerEvent::LevelsChanged { levels } => {
                            // TODO: Update VU meters if your device has them
                            let _ = levels;
                        }
                        _ => {}
                    }
                }

                // TODO: Add your hardware I/O channel here
                // Some(hw_event) = self.device_rx.recv() => {
                //     match hw_event {
                //         // Translate hardware events to D-Bus calls:
                //         // proxy.set_route_volume(input_id, output_id, volume).await.ok();
                //         // proxy.set_route_mute(input_id, output_id, muted).await.ok();
                //     }
                // }
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) {
        info!("{{device_name}} adapter shutting down");
        // TODO: Send shutdown command to your device thread
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    info!("mixctl-{{device_name}}-daemon starting");

    let runner = AdapterRunner::new();
    let shutdown = runner.shutdown_flag();

    // Signal handler
    let sf = shutdown.clone();
    tokio::spawn(async move {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        sf.store(true, Ordering::Release);
    });

    // TODO: Spawn your device I/O thread here
    // let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    // let (event_tx, event_rx) = mpsc::unbounded_channel();
    // let device_thread = std::thread::spawn(move || { ... });

    let mut adapter = MyAdapter {
        // TODO: Pass device channels to adapter
    };

    runner.run(&mut adapter).await
}
