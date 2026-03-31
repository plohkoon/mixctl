//! # mixctl Adapter SDK
//!
//! SDK for building device adapters that connect hardware controllers
//! (USB mixers, MIDI controllers, Stream Decks, etc.) to the mixctl
//! audio routing daemon.
//!
//! ## Architecture
//!
//! ```text
//!   ┌───────────────┐     D-Bus      ┌──────────────────┐
//!   │ mixctl-daemon  │◄═════════════►│  Your Adapter     │
//!   │ (PipeWire)     │  MixCtlProxy  │  (DeviceAdapter)  │
//!   └───────────────┘               │                    │
//!                                   │  ┌──────────────┐  │
//!                                   │  │ AdapterRunner │  │
//!                                   │  │ (SDK: D-Bus   │  │
//!                                   │  │  lifecycle)   │  │
//!                                   │  └──────────────┘  │
//!                                   │  ┌──────────────┐  │
//!                                   │  │ Your device   │  │
//!                                   │  │ I/O thread    │  │
//!                                   │  └──────────────┘  │
//!                                   └────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! 1. Implement [`DeviceAdapter`] for your hardware
//! 2. Create an [`AdapterRunner`] and call `runner.run(&mut adapter)`
//! 3. The SDK handles D-Bus connection, reconnection, and signal delivery
//! 4. Your `run()` method selects over mixer events + hardware I/O
//!
//! ## What the SDK does
//!
//! - Connects to the mixer daemon over D-Bus (with automatic reconnection)
//! - Registers your device and its capabilities
//! - Subscribes to all mixer signals and delivers them as [`MixerEvent`]s
//! - Provides the [`MixCtlProxy`](mixctl_core::dbus::MixCtlProxy) for calling
//!   mixer daemon methods (set_route_volume, set_output_mute, etc.)
//!
//! ## What your adapter does
//!
//! - Declares capabilities (faders, buttons, screens, LEDs, meters)
//! - Runs its own hardware I/O loop (USB polling, MIDI listening, etc.)
//! - Translates hardware input into D-Bus method calls
//! - Handles mixer state changes to update hardware (display, LEDs, etc.)

mod adapter;
mod channel;
mod runner;
mod types;

pub use adapter::DeviceAdapter;
pub use channel::channel_pair;
pub use runner::AdapterRunner;
pub use types::{
    ButtonKind, Capability, ColorMode, DeviceInput, MixerEvent, ScreenFormat,
};

// Re-export the proxy type so adapters don't need to depend on mixctl-core directly
pub use mixctl_core::dbus::MixCtlProxy;
