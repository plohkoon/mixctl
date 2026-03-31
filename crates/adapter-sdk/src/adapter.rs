use mixctl_core::dbus::MixCtlProxy;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::types::{Capability, MixerEvent};

/// Trait that all device adapters implement.
///
/// The adapter owns its hardware I/O loop. The SDK handles D-Bus lifecycle
/// (connection, reconnection, signal subscription, capability registration)
/// and delivers mixer events through a channel.
///
/// # Lifecycle
///
/// 1. `AdapterRunner` connects to D-Bus and registers the device
/// 2. `AdapterRunner` subscribes to all mixer signals, routing them as `MixerEvent`s
/// 3. `AdapterRunner` calls `run()` with the proxy and event receiver
/// 4. The adapter's `run()` selects over mixer events AND its own hardware I/O
/// 5. On return (clean shutdown or error), `AdapterRunner` calls `shutdown()`
/// 6. On D-Bus disconnect, `AdapterRunner` reconnects and calls `run()` again
///
/// # Example (minimal adapter)
///
/// ```ignore
/// struct MyAdapter { /* ... */ }
///
/// impl DeviceAdapter for MyAdapter {
///     fn capabilities(&self) -> Vec<Capability> {
///         vec![Capability::Fader { count: 8, range: (0.0, 1.0) }]
///     }
///
///     fn device_name(&self) -> &str { "my-device" }
///
///     async fn run(
///         &mut self,
///         proxy: MixCtlProxy<'static>,
///         mut mixer_events: UnboundedReceiver<MixerEvent>,
///     ) -> anyhow::Result<()> {
///         loop {
///             tokio::select! {
///                 Some(event) = mixer_events.recv() => {
///                     // handle mixer state changes
///                 }
///                 // ... also select over hardware I/O channel
///             }
///         }
///     }
///
///     async fn shutdown(&mut self) {
///         // cleanup hardware resources
///     }
/// }
/// ```
pub trait DeviceAdapter: Send + Sync {
    /// Declare what hardware capabilities this device has.
    /// Used for capability advertisement to the mixer daemon.
    fn capabilities(&self) -> Vec<Capability>;

    /// Human-readable device name used for registration.
    /// Should be unique per device type (e.g. "beacn-mix-create").
    fn device_name(&self) -> &str;

    /// Run the adapter's main loop.
    ///
    /// The adapter receives mixer events from the SDK via `mixer_events` and has
    /// full access to the D-Bus proxy for calling mixer daemon methods.
    ///
    /// The adapter should `tokio::select!` over `mixer_events` alongside its own
    /// hardware I/O (USB channel, MIDI input, HID events, etc.).
    ///
    /// Returns `Ok(())` for clean shutdown. Returns `Err` if the D-Bus connection
    /// is lost, which triggers the `AdapterRunner`'s reconnection loop.
    fn run(
        &mut self,
        proxy: MixCtlProxy<'static>,
        mixer_events: UnboundedReceiver<MixerEvent>,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;

    /// Called after `run()` returns, whether from clean shutdown or error.
    /// Use this to release hardware resources, send shutdown commands to
    /// device threads, etc.
    fn shutdown(&mut self) -> impl std::future::Future<Output = ()> + Send;
}
