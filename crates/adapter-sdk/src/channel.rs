use tokio::sync::mpsc;

/// Convenience for creating a channel pair for device thread communication.
///
/// Many USB/HID devices run their own I/O thread that pushes events through a
/// channel. This function creates the sender/receiver pair. The sender goes to
/// your device thread, the receiver stays in your adapter's `run()` method.
///
/// # Example
///
/// ```ignore
/// // In your adapter setup:
/// let (device_tx, device_rx) = channel_pair();
///
/// // Spawn your device I/O thread:
/// let device_thread = DeviceThread::spawn(device_tx);
///
/// // In your run() method, select over both channels:
/// async fn run(&mut self, proxy: MixCtlProxy<'static>, mut mixer_events: ...) {
///     loop {
///         tokio::select! {
///             Some(event) = mixer_events.recv() => { /* mixer state change */ }
///             Some(hw_event) = self.device_rx.recv() => { /* hardware input */ }
///         }
///     }
/// }
/// ```
pub fn channel_pair<T>() -> (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>) {
    mpsc::unbounded_channel()
}
