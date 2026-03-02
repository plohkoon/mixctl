# mixctl-daemon

Long-running D-Bus service that manages channel configuration and runtime state for the Beacn Mix.

## Architecture

```
main.rs ─── startup, flush loop, shutdown
config.rs ── ~/.config/mixctl.toml (channel definitions)
state.rs ─── ~/.local/state/mixctl.toml (volumes, mutes)
service.rs ── Shared struct behind Arc<Mutex>, ChannelInfo builder
dbus_adapter.rs ── #[zbus::interface] method implementations
```

### Startup sequence

1. Load config from `~/.config/mixctl.toml` (or create defaults: 4 channels with ids 1–4)
2. Fix up any missing or duplicate channel IDs (assigns next unused positive integer, logs warning)
3. Load state from `~/.local/state/mixctl.toml`
4. Reconcile state with config — add defaults for new channels, drop stale entries, clean non-numeric keys, clamp page
5. Register on the session bus as `dev.greghuber.MixCtl`
6. Start 30-second periodic flush loop for dirty config/state
7. On Ctrl-C: final flush and exit

### Config

Channel definitions with stable integer IDs. Order in the array determines display order.

```toml
version = 1

[[channels]]
id = 1
name = "System"
color = "#4A90D9"
```

The `id` field uses `Option<u32>` with `#[serde(default)]` for hand-edit resilience. On load, `fixup_ids()` assigns IDs to any channel missing one and deduplicates.

### State

Runtime values keyed by stringified channel ID (TOML map keys are always strings).

```toml
version = 1
current_page = 0

[channels.1]
muted = false
volume = 100
```

During reconciliation, entries with non-numeric keys are dropped. Missing channel IDs get default state (unmuted, volume 100).

### Dirty tracking

Config and state each have a `dirty` flag. Mutations through D-Bus set the appropriate flag. The flush loop persists dirty data every 30 seconds, plus a final flush on shutdown.

## Dependencies

- `mixctl-core` — shared types and D-Bus interface
- `zbus` — D-Bus server
- `tokio` — async runtime
- `serde` / `toml` — config and state serialization
- `tracing` — structured logging
- `dirs` — XDG-ish path resolution
