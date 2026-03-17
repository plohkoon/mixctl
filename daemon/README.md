# mixctl-daemon

Long-running D-Bus service that manages the input/output mixer configuration, runtime state, and PipeWire audio routing for the Beacn Mix.

## Architecture

```
main.rs ──────────── startup, PW engine bootstrap, flush loop, shutdown guard
config.rs ────────── ~/.config/mixctl.toml (inputs, outputs, rules)
state.rs ─────────── ~/.local/state/mixctl.toml (volumes, mutes, routes)
service.rs ───────── Shared struct behind Arc<Mutex>
shutdown.rs ──────── ShutdownGuard (Drop-based cleanup) + signal handling
dbus_adapter.rs ──── #[zbus::interface] method implementations
audio/
  mod.rs ─────────── module root, re-exports PwCommand, PwEvent, PwEngine
  engine.rs ──────── PW thread: virtual nodes, loopbacks, registry, metadata
  commands.rs ────── PwCommand enum (tokio → PW thread)
  events.rs ──────── PwEvent enum (PW thread → tokio)
  volume.rs ──────── cubic volume conversion (u8 0-100 ↔ f32 0.0-1.0)
```

### Startup sequence

1. Load config from `~/.config/mixctl.toml` (or create defaults: 4 inputs, 4 outputs)
2. Fix up any missing or duplicate IDs across inputs and outputs (shared ID space)
3. Load state from `~/.local/state/mixctl.toml`
4. Reconcile state with config — add defaults for new outputs/routes, drop stale entries, clamp page
5. Build `PwEngineConfig` using named config structs (`PwInputConfig`, `PwOutputConfig`, `PwRouteConfig`, `PwOutputTargetConfig`, `PwCaptureInputConfig`) with pre-computed combined volumes
6. Create command/event channels, spawn PipeWire engine on dedicated OS thread with shared `AtomicBool` shutdown flag
7. Start tokio relay task (forwards commands from tokio mpsc to swappable `pipewire::channel`)
8. Register on the session bus as `dev.greghuber.MixCtl`
9. Start signal emission task (caches `InterfaceRef`, dispatches via `ServiceSignal::emit()`)
10. Start event consumer task (processes PW events including `ChannelReady` for reconnection, stream auto-assignment, capture device discovery)
11. Start 30-second periodic flush loop for dirty config/state
12. Create `ShutdownGuard` with references to service, PW channel, engine, and all task handles
13. Wait for SIGINT or SIGTERM via `wait_for_signal()`
14. Drop `ShutdownGuard` → graceful shutdown sequence (see below)

### Config

Input and output definitions with stable integer IDs. Order in the arrays determines display order. IDs are shared across inputs and outputs.

```toml
version = 1
default_input = 1

[[inputs]]
id = 1
name = "System"
color = "#4A90D9"

[[outputs]]
id = 5
name = "Personal Mix"
color = "#8E44AD"
target_device = "alsa_output.pci-0000_00_1f.3.analog-stereo"

[[app_rules]]
app_name = "firefox"
input_id = 1
```

Optional fields on channels:
- `target_device`: PipeWire node name of a physical device to auto-link an output to
- `capture_device`: PipeWire node name of a hardware capture device (for capture inputs)

The `id` field uses `Option<u32>` with `#[serde(default)]` for hand-edit resilience. On load, `fixup_ids()` assigns IDs to any entry missing one and deduplicates.

### State

Runtime values keyed by stringified IDs (TOML map keys are always strings). Routes are keyed as `"input_id:output_id"`. Capture volumes are keyed by input ID.

```toml
version = 1
current_page = 0

[outputs.5]
volume = 100
muted = false

[routes."1:5"]
volume = 80
muted = false

[capture_volumes.7]
volume = 80
muted = false
```

During reconciliation:
- Output entries with non-numeric keys are dropped
- Missing outputs get default state (unmuted, volume 100)
- Missing routes (input x output pairs) get default state
- Stale output/route entries are pruned
- `current_page` is clamped to valid range

### Dirty tracking

Config and state each have a `dirty` flag. Mutations through D-Bus set the appropriate flag. The flush loop persists dirty data every 30 seconds, plus a final flush on shutdown.

## Audio engine (`audio/`)

The audio engine runs on a dedicated OS thread because PipeWire objects (`MainLoop`, `Registry`, `Node`, `Metadata`) are not `Send`/`Sync` — they must all live on the same thread.

### Thread communication

```
Tokio side                                PipeWire thread
──────────                                ───────────────
tokio::mpsc::UnboundedSender<PwCommand>   pipewire::channel::Receiver<PwCommand>
     │                                         ▲
     ▼                                         │
 relay task ── Arc<Mutex<Option<Sender>>> ──────┘
     (forwards tokio mpsc → pw channel,         (swapped on reconnect
      which wakes the PW main loop)              via ChannelReady event)

tokio::mpsc::UnboundedReceiver<PwEvent>   tokio::mpsc::UnboundedSender<PwEvent>
     ▲                                         │
     │                                         │
 event consumer task ◄─────────────────────────┘
```

- **Commands** use `pipewire::channel` because the PW main loop needs to be woken up when a command arrives. A tokio mpsc relay task bridges the two channel types. The PW channel sender is wrapped in `Arc<Mutex<Option<Sender>>>` and swapped when a `ChannelReady` event arrives after reconnection.
- **Events** use `tokio::mpsc` directly because the tokio side is always polling.
- **Reconnection**: If the PW main loop exits (PipeWire disconnects), the PW thread retries with exponential backoff (1s → 30s max). Each attempt creates a fresh `pipewire::channel` and sends `PwEvent::ChannelReady` so the relay task swaps to the new sender. Shutdown is coordinated via a shared `AtomicBool` flag checked between retry attempts.

### Virtual nodes

Created via `Core::create_object::<Node>("adapter", &props)` with `factory.name = support.null-audio-sink`:

| Purpose | `node.name` | `media.class` | Notes |
|---|---|---|---|
| Input sink | `mixctl.input.{id}` | `Audio/Sink` | Apps play audio into this |
| Output source | `mixctl.output.{id}` | `Audio/Source/Virtual` | Apps capture from this |

All virtual nodes use 8-channel (7.1) layout: `FL,FR,FC,LFE,RL,RR,SL,SR`. PipeWire handles channel adaptation automatically — stereo apps use FL/FR, surround apps use all 8.

Key properties:
- `monitor.channel-volumes = true`: allows the loopback to capture with volume
- `node.autoconnect = false`: prevents PipeWire from auto-linking to hardware
- `object.linger = false`: node is destroyed when the daemon disconnects

### Route loopbacks

Each route (input→output matrix cell) uses `libpipewire-module-loopback`, loaded via raw FFI (`pw_sys::pw_context_load_module`). There is no safe Rust wrapper for `load_module` in pipewire-rs.

The module creates an intermediate node with combined volume control (route × output):

```
[Input Sink] ──monitor──> [Loopback Node] ──playback──> [Output Source]
                            │
                         combined volume
                         (route_vol × output_vol, cubic-scaled)
```

Module arguments:
```
{
    node.name = mixctl.route.{input_id}.{output_id}
    capture.props = {
        target.object = mixctl.input.{input_id}
        stream.capture.sink = true
        audio.position = FL,FR,FC,LFE,RL,RR,SL,SR
    }
    playback.props = {
        target.object = mixctl.output.{output_id}
        channelmix.volume = {combined_pw_volume}
        audio.position = FL,FR,FC,LFE,RL,RR,SL,SR
    }
}
```

`stream.capture.sink = true` tells the loopback to capture from the sink's monitor ports (the audio being played into the sink), not from the sink's input.

The combined volume is computed on the tokio side: `combine_pw_volume(route_vol, route_muted, output_vol, output_muted)`. If either is muted, the result is `0.0`. This eliminates the need for separate mixer nodes or output-volume loopbacks.

### Module lifecycle

The raw `*mut pw_impl_module` pointer is stored in `PwLoopbackState` and manually destroyed via `pw_impl_module_destroy()` when a route is removed. Since these pointers are only used within the single PW thread, the `unsafe impl Send` is safe.

#### In-place volume updates

When a route volume changes, the daemon first tries an in-place update via SPA `set_param`. The PW engine tracks each route loopback's playback node (discovered from the registry as `Stream/Output/Audio` with `node.name` matching `mixctl.route.*`). If the playback node is available, it builds a SPA pod with `channelVolumes` (8× f32 array) and calls `node.set_param(ParamType::Props, ...)` — no module destroy/recreate needed, avoiding audio glitches during slider drags. If the playback node hasn't been discovered yet (race condition), it falls back to destroy+recreate.

### Default sink metadata

The PipeWire "default" metadata object is discovered by watching registry globals for `ObjectType::Metadata` where `props.get("metadata.name") == Some("default")`. Once found, the metadata proxy is bound via `registry.bind::<Metadata, _>(global)`.

Setting the default sink: `metadata.set_property(0, "default.audio.sink", Some("Spa:String:JSON"), Some("{\"name\": \"mixctl.input.{id}\"}"))`.

If the metadata object hasn't been discovered yet when `SetDefaultInput` is called, the request is deferred and applied once the metadata appears.

### Stream monitoring

The registry listener watches for `media.class = "Stream/Output/Audio"` nodes, filtering out any with `node.name` starting with `mixctl.` (to avoid treating our own route loopback playback nodes as app streams). When a stream appears, a `PwEvent::StreamAppeared` is sent to the tokio side, which:
1. Matches `app_name` against configured rules (exact, then glob via `glob-match`)
2. Sends `PwCommand::MoveStream` to route the stream to the matching (or default) input
3. Stores the stream in `active_streams` for D-Bus queries

Stream assignment uses the metadata system: `metadata.set_property(pw_node_id, "target.node", "Spa:String:JSON", "{\"name\": \"mixctl.input.{id}\"}")`.

### Capture device discovery

Monitors registry for `media.class = "Audio/Source"` nodes (excluding `mixctl.*` nodes). Capture devices are reported to the tokio side and stored in `capture_devices`. When added as a mixer input, a standard input sink is created plus a loopback from the hardware device to the sink, keeping the architecture uniform.

### PwCommand enum

| Command | Description |
|---|---|
| `CreateInputSink { input_id, description }` | Create virtual sink |
| `DestroyInputSink { input_id }` | Destroy virtual sink + route/capture loopbacks |
| `SetDefaultInput { input_id }` | Set PipeWire default sink |
| `RenameInputSink { input_id, description }` | Recreate sink with new name |
| `CreateOutputSource { output_id, description }` | Create virtual source |
| `DestroyOutputSource { output_id }` | Destroy source + all routes |
| `RenameOutputSource { output_id, description }` | Recreate source with new name |
| `SetRouteLink { input_id, output_id, volume: f32 }` | Create/update route loopback (combined volume) |
| `DestroyRouteLink { input_id, output_id }` | Remove route loopback |
| `SetOutputTarget { output_id, device_name }` | Link output to physical device |
| `MoveStream { pw_node_id, input_id }` | Assign stream to input |
| `CreateCaptureInput { input_id, description, capture_device_name }` | Create capture input |
| `DestroyCaptureLoopback { input_id }` | Remove capture loopback (keep input) |
| `SetCaptureVolume { input_id, pw_volume: f32 }` | Set capture loopback volume |
| `Shutdown { original_default_sink, original_stream_targets }` | Restore originals, clean teardown, quit PW loop |

`SetRouteLink.volume` is a pre-computed `f32` combining route and output volumes (both cubic-scaled). Muting is encoded as `0.0`. Output volume changes fan out as `SetRouteLink` to every route on that output.

### PwEvent enum

| Event | Description |
|---|---|
| `Connected` / `Disconnected` | PipeWire connection lifecycle |
| `ChannelReady { sender }` | PW thread created new channel after (re)connect |
| `InputSinkCreated { input_id, pw_node_id }` | Sink ready |
| `InputSinkDestroyed { input_id }` | Sink removed |
| `OutputSourceCreated { output_id, pw_node_id }` | Source ready |
| `OutputSourceDestroyed { output_id }` | Source removed |
| `RouteLinkCreated { input_id, output_id }` | Loopback ready |
| `StreamAppeared { pw_node_id, app_name, media_name }` | App started playing |
| `StreamRemoved { pw_node_id }` | App stopped |
| `CaptureDeviceAppeared { pw_node_id, name, device_name }` | Mic/line-in detected |
| `CaptureDeviceRemoved { pw_node_id }` | Device unplugged |
| `OriginalDefaultSink { value }` | Pre-daemon default sink (for shutdown restore) |
| `OriginalStreamTarget { stream_id, value }` | Pre-daemon stream target (for shutdown restore) |
| `Error { message }` | PipeWire error |

`ChannelReady` is handled directly in `main.rs` (swaps the relay sender); all other events go through `Service::handle_pw_event()`. `PwEvent` has a manual `Debug` impl because `pipewire::channel::Sender` doesn't implement `Debug`.

### Graceful shutdown

The daemon uses a `ShutdownGuard` (Drop-based) pattern to ensure cleanup runs on normal exit, error, or panic. Original PipeWire state is captured on the PW thread via a metadata listener and sent as events to the tokio side, where it's stored in `Shared`.

**State capture**: A metadata property listener is registered when the "default" metadata object is discovered. It fires for every `default.audio.sink` and `target.node` change. The tokio-side handler uses `is_none()` / `or_insert()` guards so only the first (pre-daemon) value is saved — subsequent changes from the daemon's own overrides are ignored.

**Shutdown sequence** (triggered by SIGINT, SIGTERM, error, or panic):

```
ShutdownGuard::drop()
  │
  ├─ Lock Shared, persist stream→input assignments as app rules
  ├─ Build Shutdown command with original_default_sink + original_stream_targets
  ├─ shutdown_flag.store(true)
  ├─ Send Shutdown directly via pw_chan_tx (bypasses relay)
  │       │
  │       ▼ (PW thread)
  │       ├─ Restore default.audio.sink to pre-daemon value
  │       ├─ Restore/clear target.node for each known stream
  │       ├─ Destroy capture/route/output-target loopbacks
  │       ├─ Drop input sinks + output sources
  │       └─ main_loop.quit()
  │
  ├─ Abort tokio tasks
  ├─ pw_engine.join()
  └─ Final flush config/state to disk
```

**Crash resilience**:
- **PW thread crash** → tokio side still has originals in `Shared`. PW reconnects automatically, cleanup uses saved values.
- **Main thread crash** → `ShutdownGuard` Drop impl reads originals from `Shared` (via `try_lock`) and sends them with the Shutdown command to the PW thread.
- **Setup failure** → no guard exists yet, no audio has been intercepted. Process exit causes PipeWire to clean up `object.linger=false` nodes automatically.

**Stream assignment persistence**: On shutdown, `persist_stream_assignments()` saves current stream→input mappings as app rules for any stream that doesn't already have a matching rule. On restart, these rules auto-assign returning streams to their previous inputs.

## Dependencies

- `mixctl-core` — shared types and D-Bus interface
- `pipewire` (0.9) — PipeWire client bindings (MainLoop, Context, Core, Registry, Node, Metadata, channel)
- `glob-match` — glob pattern matching for app rules
- `zbus` — D-Bus server
- `tokio` — async runtime
- `serde` / `toml` — config and state serialization
- `tracing` — structured logging
- `dirs` — XDG-ish path resolution
