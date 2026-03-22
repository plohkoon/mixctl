use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use pipewire as pw;
use pipewire::core::CoreRc;
use pipewire::link::Link;
use pipewire::node::Node;
use pipewire::properties::properties;
use pipewire::spa::param::ParamType;
use pipewire::spa::pod::serialize::PodSerializer;
use pipewire::spa::pod::{Object, Pod, Property, Value, ValueArray};
use pipewire::spa::utils::SpaTypes;
use pipewire::types::ObjectType;
use tracing::{debug, error, info, warn};

use super::commands::PwCommand;
use super::events::PwEvent;
use super::mixer::{MixerFilter, VolumeMatrix, CHANNELS, NUM_CHANNELS};

/// SPA_PROP_channelVolumes (0x20005 from spa/param/props.h)
const SPA_PROP_CHANNEL_VOLUMES: u32 = 0x20005;

/// SPA_PROP_monitorVolumes (0x10001 from spa/param/props.h)
const SPA_PROP_MONITOR_VOLUMES: u32 = 0x10001;

/// Minimum interval between level updates per node (~20Hz)
const LEVEL_THROTTLE_MS: u128 = 50;

pub struct PwEngine {
    thread_handle: thread::JoinHandle<()>,
}

/// Initial state to bootstrap the PipeWire thread.
pub struct PwEngineConfig {
    pub inputs: Vec<PwInputConfig>,
    pub outputs: Vec<PwOutputConfig>,
    pub routes: Vec<PwRouteConfig>,
    pub output_targets: Vec<PwOutputTargetConfig>,
    pub capture_inputs: Vec<PwCaptureInputConfig>,
    pub default_input_id: Option<u32>,
    pub broadcast_levels: bool,
}

pub struct PwInputConfig {
    pub input_id: u32,
    pub description: String,
}

pub struct PwOutputConfig {
    pub output_id: u32,
    pub description: String,
}

pub struct PwRouteConfig {
    pub input_id: u32,
    pub output_id: u32,
    pub pw_volume: f32,
}

pub struct PwOutputTargetConfig {
    pub output_id: u32,
    pub device_name: String,
}

pub struct PwCaptureInputConfig {
    pub input_id: u32,
    pub capture_device_name: String,
}

impl PwEngine {
    pub fn spawn(
        config: PwEngineConfig,
        shutdown_flag: Arc<AtomicBool>,
        event_sender: tokio::sync::mpsc::UnboundedSender<PwEvent>,
    ) -> Self {
        let thread_handle = thread::spawn(move || {
            pw::init();
            let mut backoff = Duration::from_secs(1);
            loop {
                let (new_tx, new_rx) = pipewire::channel::channel();
                event_sender.send(PwEvent::ChannelReady { sender: new_tx }).ok();
                run_pw_loop(&config, new_rx, &event_sender);
                if shutdown_flag.load(Ordering::Acquire) {
                    break;
                }
                warn!("PipeWire disconnected, reconnecting in {:?}", backoff);
                thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
            info!("PipeWire thread exiting");
        });
        PwEngine { thread_handle }
    }

    pub fn join(self) {
        if let Err(e) = self.thread_handle.join() {
            error!("PipeWire thread panicked: {e:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// PendingLink — declarative link spec resolved via registry
// ---------------------------------------------------------------------------

/// A link we want to create, identified by node name + port name on each side.
/// Resolved to numeric port IDs when both ports appear in the registry.
#[derive(Clone, Debug)]
struct PendingLink {
    /// Group key for bulk operations (e.g. "input_to_filter:1")
    group: String,
    out_node_name: String,
    out_port_name: String,
    in_node_name: String,
    in_port_name: String,
}

// ---------------------------------------------------------------------------
// run_pw_loop
// ---------------------------------------------------------------------------

fn run_pw_loop(
    config: &PwEngineConfig,
    cmd_receiver: pipewire::channel::Receiver<PwCommand>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
) {
    let main_loop = match pw::main_loop::MainLoopRc::new(None) {
        Ok(ml) => ml,
        Err(e) => {
            error!("failed to create PipeWire MainLoop: {e}");
            event_sender.send(PwEvent::Error {
                message: format!("failed to create MainLoop: {e}"),
            }).ok();
            return;
        }
    };

    let context = match pw::context::ContextRc::new(&main_loop, None) {
        Ok(ctx) => ctx,
        Err(e) => {
            error!("failed to create PipeWire Context: {e}");
            event_sender.send(PwEvent::Error {
                message: format!("failed to create Context: {e}"),
            }).ok();
            return;
        }
    };

    let core = match context.connect_rc(None) {
        Ok(c) => c,
        Err(e) => {
            error!("failed to connect to PipeWire: {e}");
            event_sender.send(PwEvent::Error {
                message: format!("failed to connect to PipeWire: {e}"),
            }).ok();
            return;
        }
    };

    let registry = core.get_registry_rc().expect("failed to get registry");

    let volume_matrix = Arc::new(VolumeMatrix::new());

    let mixer = MixerFilter::new(&core, Arc::clone(&volume_matrix));
    if mixer.is_none() {
        error!("failed to create mixer filter, aborting PW loop");
        event_sender.send(PwEvent::Error {
            message: "failed to create mixer filter".into(),
        }).ok();
        return;
    }

    let state = Rc::new(RefCell::new(PwState {
        core: Some(core.clone()),
        input_sinks: HashMap::new(),
        input_sink_pw_ids: HashMap::new(),
        output_sources: HashMap::new(),
        mixer: mixer,
        volume_matrix,
        input_id_to_idx: HashMap::new(),
        output_id_to_idx: HashMap::new(),
        // Port tracking for registry-driven linking
        node_name_to_id: HashMap::new(),
        port_index: HashMap::new(),
        port_id_to_key: HashMap::new(),
        pending_links: Vec::new(),
        active_links: HashMap::new(),
        // Stream routing
        pending_stream_routes: HashMap::new(),
        stream_port_cache: HashMap::new(),
        // Capture device names for volume changes
        capture_device_names: HashMap::new(),
        metadata: None,
        _metadata_listener: None,
        deferred_default_input: config.default_input_id,
        routing_ready: false,
        deferred_stream_moves: Vec::new(),
        routing_wait_start: None,
        known_stream_ids: HashSet::new(),
        known_capture_ids: HashSet::new(),
        known_playback_ids: HashSet::new(),
        shutdown: false,
        level_monitoring: config.broadcast_levels,
        level_listeners: HashMap::new(),
        level_peaks: HashMap::new(),
        level_last_sent: Instant::now(),
    }));

    // Registry listener
    let reg_state = state.clone();
    let reg_event_sender = event_sender.clone();
    let reg_registry = registry.clone();
    let _reg_listener = registry
        .add_listener_local()
        .global(move |global| {
            handle_registry_global(&reg_state, &reg_event_sender, &reg_registry, global);
        })
        .global_remove({
            let ev = event_sender.clone();
            let remove_state = state.clone();
            move |id| {
                let mut s = remove_state.borrow_mut();
                // Clean up port tracking
                if let Some(key) = s.port_id_to_key.remove(&id) {
                    s.port_index.remove(&key);
                }
                // Clean up node tracking
                s.node_name_to_id.retain(|_, v| *v != id);
                // Clean up stream/device tracking
                if s.known_stream_ids.remove(&id) {
                    s.active_links.retain(|k, _| !k.starts_with(&format!("stream:{id}:")));
                    s.pending_links.retain(|pl| !pl.group.starts_with(&format!("stream:{id}")));
                    s.pending_stream_routes.remove(&id);
                    s.stream_port_cache.remove(&id);
                    ev.send(PwEvent::StreamRemoved { pw_node_id: id }).ok();
                } else if s.known_capture_ids.remove(&id) {
                    ev.send(PwEvent::CaptureDeviceRemoved { pw_node_id: id }).ok();
                } else if s.known_playback_ids.remove(&id) {
                    ev.send(PwEvent::PlaybackDeviceRemoved { pw_node_id: id }).ok();
                }
            }
        })
        .register();

    // Core error listener
    let error_event_sender = event_sender.clone();
    let _core_listener = core
        .add_listener_local()
        .error(move |_id, _seq, _res, msg| {
            if msg.contains("File exists") {
                debug!("PipeWire link already exists: {msg}");
            } else {
                error!("PipeWire core error: {msg}");
                error_event_sender.send(PwEvent::Error {
                    message: msg.to_string(),
                }).ok();
            }
        })
        .register();

    // Command receiver
    let cmd_state = state.clone();
    let cmd_event_sender = event_sender.clone();
    let cmd_main_loop = main_loop.clone();
    let cmd_core = core.clone();
    let _cmd_receiver = cmd_receiver.attach(main_loop.loop_(), move |cmd| {
        handle_command(&cmd_state, &cmd_event_sender, &cmd_main_loop, &cmd_core, cmd);
    });

    event_sender.send(PwEvent::Connected).ok();
    info!("PipeWire connected");

    // Create initial sinks/sources (ports will be tracked via registry)
    for cfg in &config.inputs {
        create_input_sink(&state, &event_sender, &core, cfg.input_id, &cfg.description);
    }
    for cfg in &config.outputs {
        create_output_source(&state, &event_sender, &core, cfg.output_id, &cfg.description);
    }

    // Connect the mixer filter
    {
        let s = state.borrow();
        if let Some(ref mixer) = s.mixer {
            mixer.connect();
        }
    }

    // Queue pending links — they'll resolve when ports appear in the registry
    queue_input_to_filter_links(&state, &config.inputs.iter().map(|c| c.input_id).collect::<Vec<_>>());
    queue_filter_to_output_links(&state, &config.outputs.iter().map(|c| c.output_id).collect::<Vec<_>>());
    for cfg in &config.output_targets {
        queue_output_target_links(&state, cfg.output_id, &cfg.device_name);
    }
    // Start routing readiness timer if we have output targets to wait for
    {
        let mut s = state.borrow_mut();
        if config.output_targets.is_empty() {
            s.routing_ready = true;
            debug!("no output targets configured, routing immediately ready");
        } else {
            s.routing_wait_start = Some(Instant::now());
        }
    }
    for cfg in &config.capture_inputs {
        queue_capture_links(&state, cfg.input_id, &cfg.capture_device_name);
    }

    // Set initial volume matrix
    {
        let s = state.borrow();
        info!("index maps: inputs={:?} outputs={:?}", s.input_id_to_idx, s.output_id_to_idx);
    }
    for cfg in &config.routes {
        let s = state.borrow();
        if let (Some(&iidx), Some(&oidx)) = (
            s.input_id_to_idx.get(&cfg.input_id),
            s.output_id_to_idx.get(&cfg.output_id),
        ) {
            s.volume_matrix.set(iidx, oidx, cfg.pw_volume);
            info!("volume matrix[{iidx},{oidx}] = {} (input {} → output {})", cfg.pw_volume, cfg.input_id, cfg.output_id);
        } else {
            warn!("cannot set volume: input {} or output {} not in index map", cfg.input_id, cfg.output_id);
        }
        drop(s);
        event_sender.send(PwEvent::RouteLinkCreated {
            input_id: cfg.input_id,
            output_id: cfg.output_id,
        }).ok();
    }

    main_loop.run();

    if !state.borrow().shutdown {
        event_sender.send(PwEvent::Disconnected).ok();
    }
    info!("PipeWire main loop exited");
}

// ---------------------------------------------------------------------------
// PwState
// ---------------------------------------------------------------------------

struct PwState {
    core: Option<CoreRc>,
    input_sinks: HashMap<u32, PwNodeState>,
    input_sink_pw_ids: HashMap<u32, u32>,
    output_sources: HashMap<u32, PwNodeState>,
    mixer: Option<MixerFilter>,
    volume_matrix: Arc<VolumeMatrix>,
    input_id_to_idx: HashMap<u32, usize>,
    output_id_to_idx: HashMap<u32, usize>,

    /// Node name → PW node ID (for all nodes we care about)
    node_name_to_id: HashMap<String, u32>,
    /// (node_id, port_name) → port global ID
    port_index: HashMap<(u32, String), u32>,
    /// Reverse: port global ID → (node_id, port_name) for cleanup
    port_id_to_key: HashMap<u32, (u32, String)>,
    /// Links waiting for both ports to appear in the registry
    pending_links: Vec<PendingLink>,
    /// Created links, grouped by key prefix (e.g. "input_to_filter:1")
    active_links: HashMap<String, Vec<Link>>,

    pending_stream_routes: HashMap<u32, u32>,
    stream_port_cache: HashMap<u32, Vec<(u32, String)>>,
    /// Capture device names for volume changes (input_id → device_name)
    capture_device_names: HashMap<u32, String>,

    metadata: Option<pw::metadata::Metadata>,
    _metadata_listener: Option<Box<dyn std::any::Any>>,
    deferred_default_input: Option<u32>,
    /// True once all output→hardware links are active and stream routing is safe.
    /// Prevents yanking streams off hardware sinks before the mixer chain is ready.
    routing_ready: bool,
    /// Streams waiting to be routed once routing_ready becomes true.
    deferred_stream_moves: Vec<(u32, u32)>,
    /// When we started waiting for routing readiness (for timeout).
    routing_wait_start: Option<Instant>,
    known_stream_ids: HashSet<u32>,
    known_capture_ids: HashSet<u32>,
    known_playback_ids: HashSet<u32>,
    shutdown: bool,
    level_monitoring: bool,
    level_listeners: HashMap<u32, Box<dyn std::any::Any>>,
    level_peaks: HashMap<u32, f32>,
    level_last_sent: Instant,
}

struct PwNodeState {
    _proxy: Node,
    _listener: Box<dyn std::any::Any>,
}

// ---------------------------------------------------------------------------
// Link resolution
// ---------------------------------------------------------------------------

/// Try to resolve and create any pending links whose ports are now known.
fn try_resolve_pending_links(state: &Rc<RefCell<PwState>>) {
    let (pending, core) = {
        let s = state.borrow();
        let core = match &s.core {
            Some(c) => c.clone(),
            None => return,
        };
        (s.pending_links.clone(), core)
    };

    if pending.is_empty() {
        return;
    }

    let mut still_pending = Vec::new();
    let mut new_links: Vec<(String, Link)> = Vec::new();

    for pl in pending {
        let resolved = {
            let s = state.borrow();
            resolve_link_ports(&s, &pl)
        };

        match resolved {
            Some((out_port_id, in_port_id)) => {
                if let Some(link) = create_link_by_id(&core, out_port_id, in_port_id) {
                    new_links.push((pl.group, link));
                }
            }
            None => {
                still_pending.push(pl);
            }
        }
    }

    if !new_links.is_empty() {
        debug!("resolved {} links ({} still pending)", new_links.len(), still_pending.len());
    }

    let mut s = state.borrow_mut();
    s.pending_links = still_pending;
    for (group, link) in new_links {
        s.active_links.entry(group).or_default().push(link);
    }
    drop(s);
    try_activate_routing(state);
}

/// Check if all output→hardware links are resolved and activate stream routing.
///
/// This prevents yanking streams off hardware sinks (like USB DACs) before the
/// full mixer chain is connected. The XMOS SDAC firmware can lock up if a stream
/// is abruptly interrupted and restarted with different parameters.
fn try_activate_routing(state: &Rc<RefCell<PwState>>) {
    let mut s = state.borrow_mut();
    if s.routing_ready {
        return;
    }
    if s.metadata.is_none() {
        return;
    }

    let has_pending_targets = s
        .pending_links
        .iter()
        .any(|pl| pl.group.starts_with("output_target:"));

    // Allow a 3-second timeout for targets that may never resolve (unplugged devices)
    let timed_out = s
        .routing_wait_start
        .map(|t| t.elapsed() > Duration::from_secs(3))
        .unwrap_or(false);

    if has_pending_targets && !timed_out {
        return;
    }

    if has_pending_targets {
        warn!("output target links not fully resolved after 3s, activating routing anyway");
    }

    s.routing_ready = true;
    info!("output chain ready, activating stream routing");

    // Apply deferred default input
    if let Some(input_id) = s.deferred_default_input.take() {
        if let Some(metadata) = &s.metadata {
            let target = format!("mixctl.input.{input_id}");
            let value = format!("{{\"name\": \"{target}\"}}");
            metadata.set_property(
                0,
                "default.audio.sink",
                Some("Spa:String:JSON"),
                Some(&value),
            );
            debug!("set default audio sink to input {input_id}");
        }
    }

    // Route deferred streams
    let deferred = std::mem::take(&mut s.deferred_stream_moves);
    drop(s);

    for (pw_node_id, input_id) in deferred {
        debug!("routing deferred stream {pw_node_id} to input {input_id}");
        execute_move_stream(state, pw_node_id, input_id);
    }
}

/// Resolve a pending link to (output_port_global_id, input_port_global_id).
fn resolve_link_ports(s: &PwState, pl: &PendingLink) -> Option<(u32, u32)> {
    let out_node_id = s.node_name_to_id.get(&pl.out_node_name)?;
    let in_node_id = s.node_name_to_id.get(&pl.in_node_name)?;
    let out_port_id = s.port_index.get(&(*out_node_id, pl.out_port_name.clone()))?;
    let in_port_id = s.port_index.get(&(*in_node_id, pl.in_port_name.clone()))?;
    Some((*out_port_id, *in_port_id))
}

/// Create a link using numeric port IDs (globally unique, no name resolution needed).
fn create_link_by_id(core: &CoreRc, output_port_id: u32, input_port_id: u32) -> Option<Link> {
    let props = properties! {
        "link.output.port" => output_port_id.to_string(),
        "link.input.port" => input_port_id.to_string(),
        "object.linger" => "false",
    };
    match core.create_object::<Link>("link-factory", &props) {
        Ok(link) => Some(link),
        Err(e) => {
            warn!("failed to create link (out_port={output_port_id}, in_port={input_port_id}): {e}");
            None
        }
    }
}

/// Execute the MoveStream logic: remove old links, create new links, set metadata.
fn execute_move_stream(
    state: &Rc<RefCell<PwState>>,
    pw_node_id: u32,
    input_id: u32,
) {
    {
        let mut s = state.borrow_mut();
        remove_link_group(&mut s, &format!("stream:{pw_node_id}"));

        if let Some(ports) = s.stream_port_cache.get(&pw_node_id) {
            let ports_snapshot = ports.clone();
            drop(s);
            queue_stream_links(state, pw_node_id, input_id, &ports_snapshot);
            try_resolve_pending_links(state);
        } else {
            s.pending_stream_routes.insert(pw_node_id, input_id);
        }
    }
    // Metadata hint for WirePlumber
    let s = state.borrow();
    if let Some(metadata) = &s.metadata {
        let target_name = format!("mixctl.input.{input_id}");
        metadata.set_property(
            pw_node_id,
            "target.object",
            Some("Spa:String:JSON"),
            Some(&target_name),
        );
        let value = format!("{{\"name\": \"{target_name}\"}}");
        metadata.set_property(
            pw_node_id,
            "target.node",
            Some("Spa:String:JSON"),
            Some(&value),
        );
    }
}

/// Remove all active and pending links matching a group prefix.
fn remove_link_group(s: &mut PwState, prefix: &str) {
    s.active_links.retain(|k, _| !k.starts_with(prefix));
    s.pending_links.retain(|pl| !pl.group.starts_with(prefix));
}

// ---------------------------------------------------------------------------
// Link queueing helpers
// ---------------------------------------------------------------------------

fn queue_input_to_filter_links(state: &Rc<RefCell<PwState>>, input_ids: &[u32]) {
    let mut s = state.borrow_mut();
    for &input_id in input_ids {
        for ch in &CHANNELS {
            s.pending_links.push(PendingLink {
                group: format!("input_to_filter:{input_id}"),
                out_node_name: format!("mixctl.input.{input_id}"),
                out_port_name: format!("monitor_{ch}"),
                in_node_name: "mixctl.mixer".to_string(),
                in_port_name: format!("in_{input_id}_{ch}"),
            });
        }
    }
}

fn queue_filter_to_output_links(state: &Rc<RefCell<PwState>>, output_ids: &[u32]) {
    let mut s = state.borrow_mut();
    for &output_id in output_ids {
        for ch in &CHANNELS {
            s.pending_links.push(PendingLink {
                group: format!("filter_to_output:{output_id}"),
                out_node_name: "mixctl.mixer".to_string(),
                out_port_name: format!("out_{output_id}_{ch}"),
                in_node_name: format!("mixctl.output.{output_id}"),
                in_port_name: format!("playback_{ch}"),
            });
        }
    }
}

fn queue_output_target_links(state: &Rc<RefCell<PwState>>, output_id: u32, device_name: &str) {
    let mut s = state.borrow_mut();
    for ch in &CHANNELS {
        s.pending_links.push(PendingLink {
            group: format!("output_target:{output_id}"),
            out_node_name: format!("mixctl.output.{output_id}"),
            out_port_name: format!("monitor_{ch}"),
            in_node_name: device_name.to_string(),
            in_port_name: format!("playback_{ch}"),
        });
    }
}

fn queue_capture_links(state: &Rc<RefCell<PwState>>, input_id: u32, capture_device_name: &str) {
    let mut s = state.borrow_mut();
    s.capture_device_names.insert(input_id, capture_device_name.to_string());
    for ch in &["FL", "FR"] {
        s.pending_links.push(PendingLink {
            group: format!("capture:{input_id}"),
            out_node_name: capture_device_name.to_string(),
            out_port_name: format!("capture_{ch}"),
            in_node_name: format!("mixctl.input.{input_id}"),
            in_port_name: format!("playback_{ch}"),
        });
    }
}

fn queue_stream_links(state: &Rc<RefCell<PwState>>, pw_node_id: u32, input_id: u32, ports: &[(u32, String)]) {
    let mut s = state.borrow_mut();
    // Use the numeric node ID as the "node name" for streams
    let node_name = pw_node_id.to_string();
    // Ensure the node is in our name→id map
    s.node_name_to_id.insert(node_name.clone(), pw_node_id);
    for (_port_global_id, port_name) in ports {
        if let Some(ch) = port_name.strip_prefix("output_") {
            s.pending_links.push(PendingLink {
                group: format!("stream:{pw_node_id}"),
                out_node_name: node_name.clone(),
                out_port_name: port_name.clone(),
                in_node_name: format!("mixctl.input.{input_id}"),
                in_port_name: format!("playback_{ch}"),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Registry handlers
// ---------------------------------------------------------------------------

fn handle_registry_global(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    registry: &pw::registry::Registry,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
) {
    match global.type_ {
        ObjectType::Metadata => handle_metadata_global(state, event_sender, registry, global),
        ObjectType::Node => handle_node_global(state, event_sender, global),
        ObjectType::Port => handle_port_global(state, global),
        _ => {}
    }
}

fn handle_metadata_global(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    registry: &pw::registry::Registry,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
) {
    let props = match &global.props {
        Some(p) => p,
        None => return,
    };
    if props.get("metadata.name") != Some("default") {
        return;
    }
    let mut s = state.borrow_mut();
    if s.metadata.is_some() {
        return;
    }
    let metadata = match registry.bind::<pw::metadata::Metadata, _>(global) {
        Ok(m) => m,
        Err(e) => { warn!("failed to bind metadata: {e}"); return; }
    };
    debug!("bound default metadata object (id={})", global.id);

    let listener = metadata
        .add_listener_local()
        .property({
            let event_sender = event_sender.clone();
            move |subject, key, _type, value| {
                match (subject, key) {
                    (0, Some("default.audio.sink")) => {
                        event_sender.send(PwEvent::OriginalDefaultSink {
                            value: value.map(|v| v.to_string()),
                        }).ok();
                    }
                    (id, Some("target.node")) if id != 0 => {
                        if let Some(v) = value {
                            if !v.contains("mixctl.") {
                                event_sender.send(PwEvent::OriginalStreamTarget {
                                    stream_id: id,
                                    value: v.to_string(),
                                }).ok();
                            }
                        }
                    }
                    _ => {}
                }
                0
            }
        })
        .register();
    s._metadata_listener = Some(Box::new(listener));

    // Don't apply deferred_default_input yet — wait for output→hardware links
    // to resolve first, preventing an abrupt stream yank that can crash USB DAC firmware.
    if s.deferred_default_input.is_some() && s.routing_wait_start.is_none() {
        s.routing_wait_start = Some(Instant::now());
    }
    s.metadata = Some(metadata);
    drop(s);
    try_activate_routing(state);
}

fn handle_node_global(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
) {
    let props = match &global.props {
        Some(p) => p,
        None => return,
    };
    let node_name = props.get("node.name").unwrap_or("");
    let media_class = props.get("media.class").unwrap_or("");

    // Always track node name → ID for our own nodes (needed for link resolution)
    if node_name.starts_with("mixctl.") {
        state.borrow_mut().node_name_to_id.insert(node_name.to_string(), global.id);
        // Try resolving pending links now that we know this node's ID
        try_resolve_pending_links(state);
        return;
    }

    match media_class {
        "Stream/Output/Audio" => {
            let effective_name = node_name.strip_prefix("output.").unwrap_or(node_name);
            if effective_name.starts_with("mixctl.") {
                return;
            }
            let app_name = props
                .get("application.name")
                .or_else(|| props.get("node.name"))
                .unwrap_or("unknown")
                .to_string();
            let media_name = props.get("media.name").unwrap_or("").to_string();
            let mut s = state.borrow_mut();
            s.known_stream_ids.insert(global.id);
            // Map numeric node ID so stream links can resolve
            s.node_name_to_id.insert(global.id.to_string(), global.id);
            drop(s);
            event_sender.send(PwEvent::StreamAppeared {
                pw_node_id: global.id,
                app_name,
                media_name,
            }).ok();
        }
        "Stream/Input/Audio" => {}
        "Audio/Source" => {
            let name = props
                .get("node.description")
                .or_else(|| props.get("node.nick"))
                .unwrap_or(node_name)
                .to_string();
            let mut s = state.borrow_mut();
            s.known_capture_ids.insert(global.id);
            s.node_name_to_id.insert(node_name.to_string(), global.id);
            drop(s);
            event_sender.send(PwEvent::CaptureDeviceAppeared {
                pw_node_id: global.id,
                name,
                device_name: node_name.to_string(),
            }).ok();
            try_resolve_pending_links(state);
        }
        "Audio/Sink" => {
            let name = props
                .get("node.description")
                .or_else(|| props.get("node.nick"))
                .unwrap_or(node_name)
                .to_string();
            let mut s = state.borrow_mut();
            s.known_playback_ids.insert(global.id);
            s.node_name_to_id.insert(node_name.to_string(), global.id);
            drop(s);
            event_sender.send(PwEvent::PlaybackDeviceAppeared {
                pw_node_id: global.id,
                name,
                device_name: node_name.to_string(),
            }).ok();
            try_resolve_pending_links(state);
        }
        _ => {}
    }
}

/// Track port globals for registry-driven linking and stream port discovery.
fn handle_port_global(
    state: &Rc<RefCell<PwState>>,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
) {
    let props = match &global.props {
        Some(p) => p,
        None => return,
    };
    let node_id: u32 = match props.get("node.id").and_then(|v| v.parse().ok()) {
        Some(id) => id,
        None => return,
    };
    let port_name = match props.get("port.name") {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => return,
    };

    let mut s = state.borrow_mut();

    // Store port in the index
    let key = (node_id, port_name.clone());
    s.port_index.insert(key.clone(), global.id);
    s.port_id_to_key.insert(global.id, key);

    // Stream port discovery: if this port belongs to a pending stream route,
    // cache it and check if we have enough to queue stream links
    let port_direction = props.get("port.direction").unwrap_or("");
    if port_direction == "out" {
        if let Some(&target_input_id) = s.pending_stream_routes.get(&node_id) {
            let ports = s.stream_port_cache.entry(node_id).or_default();
            ports.push((global.id, port_name));

            let has_fl = ports.iter().any(|(_, n)| n == "output_FL");
            let has_fr = ports.iter().any(|(_, n)| n == "output_FR");
            let port_count = ports.len();

            if (has_fl && has_fr) || port_count >= NUM_CHANNELS {
                let ports_snapshot = ports.clone();
                let pw_node_id = node_id;
                s.pending_stream_routes.remove(&node_id);
                drop(s);
                queue_stream_links(state, pw_node_id, target_input_id, &ports_snapshot);
                try_resolve_pending_links(state);
                return;
            }
        }
    }

    drop(s);
    try_resolve_pending_links(state);
}

// ---------------------------------------------------------------------------
// Command handler
// ---------------------------------------------------------------------------

fn handle_command(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    main_loop: &pw::main_loop::MainLoop,
    core: &CoreRc,
    cmd: PwCommand,
) {
    match cmd {
        PwCommand::CreateInputSink { input_id, description } => {
            create_input_sink(state, event_sender, core, input_id, &description);
            queue_input_to_filter_links(state, &[input_id]);
            try_resolve_pending_links(state);
        }

        PwCommand::DestroyInputSink { input_id } => {
            let mut s = state.borrow_mut();
            remove_link_group(&mut s, &format!("input_to_filter:{input_id}"));
            remove_link_group(&mut s, &format!("capture:{input_id}"));
            s.capture_device_names.remove(&input_id);
            if let Some(&iidx) = s.input_id_to_idx.get(&input_id) {
                for oidx in 0..s.output_id_to_idx.len() {
                    s.volume_matrix.set(iidx, oidx, 0.0);
                }
                if let Some(ref mut mixer) = s.mixer {
                    mixer.remove_input_ports(iidx);
                }
                let removed_idx = iidx;
                s.input_id_to_idx.remove(&input_id);
                for (_, v) in s.input_id_to_idx.iter_mut() {
                    if *v > removed_idx { *v -= 1; }
                }
            }
            s.level_listeners.remove(&input_id);
            s.level_peaks.remove(&input_id);
            s.input_sink_pw_ids.remove(&input_id);
            if s.input_sinks.remove(&input_id).is_some() {
                event_sender.send(PwEvent::InputSinkDestroyed { input_id }).ok();
            }
        }

        PwCommand::SetDefaultInput { input_id } => {
            set_default_input(state, input_id);
        }

        PwCommand::RenameInputSink { input_id, description } => {
            let mut s = state.borrow_mut();
            let existed = s.input_sinks.remove(&input_id).is_some();
            remove_link_group(&mut s, &format!("input_to_filter:{input_id}"));
            drop(s);
            if existed {
                create_input_sink(state, event_sender, core, input_id, &description);
                queue_input_to_filter_links(state, &[input_id]);
                try_resolve_pending_links(state);
            }
        }

        PwCommand::CreateOutputSource { output_id, description } => {
            create_output_source(state, event_sender, core, output_id, &description);
            queue_filter_to_output_links(state, &[output_id]);
            try_resolve_pending_links(state);
        }

        PwCommand::DestroyOutputSource { output_id } => {
            let mut s = state.borrow_mut();
            remove_link_group(&mut s, &format!("filter_to_output:{output_id}"));
            remove_link_group(&mut s, &format!("output_target:{output_id}"));
            if let Some(&oidx) = s.output_id_to_idx.get(&output_id) {
                for iidx in 0..s.input_id_to_idx.len() {
                    s.volume_matrix.set(iidx, oidx, 0.0);
                }
                if let Some(ref mut mixer) = s.mixer {
                    mixer.remove_output_ports(oidx);
                }
                let removed_idx = oidx;
                s.output_id_to_idx.remove(&output_id);
                for (_, v) in s.output_id_to_idx.iter_mut() {
                    if *v > removed_idx { *v -= 1; }
                }
            }
            if s.output_sources.remove(&output_id).is_some() {
                event_sender.send(PwEvent::OutputSourceDestroyed { output_id }).ok();
            }
        }

        PwCommand::RenameOutputSource { output_id, description } => {
            let mut s = state.borrow_mut();
            let existed = s.output_sources.remove(&output_id).is_some();
            remove_link_group(&mut s, &format!("filter_to_output:{output_id}"));
            drop(s);
            if existed {
                create_output_source(state, event_sender, core, output_id, &description);
                queue_filter_to_output_links(state, &[output_id]);
                try_resolve_pending_links(state);
            }
        }

        PwCommand::SetRouteLink { input_id, output_id, volume } => {
            let s = state.borrow();
            if let (Some(&iidx), Some(&oidx)) = (
                s.input_id_to_idx.get(&input_id),
                s.output_id_to_idx.get(&output_id),
            ) {
                s.volume_matrix.set(iidx, oidx, volume);
            }
            drop(s);
            event_sender.send(PwEvent::RouteLinkCreated { input_id, output_id }).ok();
        }

        PwCommand::DestroyRouteLink { input_id, output_id } => {
            let s = state.borrow();
            if let (Some(&iidx), Some(&oidx)) = (
                s.input_id_to_idx.get(&input_id),
                s.output_id_to_idx.get(&output_id),
            ) {
                s.volume_matrix.set(iidx, oidx, 0.0);
            }
        }

        PwCommand::SetOutputTarget { output_id, device_name } => {
            remove_link_group(&mut state.borrow_mut(), &format!("output_target:{output_id}"));
            if let Some(device) = device_name {
                queue_output_target_links(state, output_id, &device);
                try_resolve_pending_links(state);
            }
        }

        PwCommand::MoveStream { pw_node_id, input_id } => {
            let mut s = state.borrow_mut();
            if !s.routing_ready {
                debug!("deferring stream {pw_node_id} routing until output chain ready");
                s.deferred_stream_moves.push((pw_node_id, input_id));
            } else {
                drop(s);
                execute_move_stream(state, pw_node_id, input_id);
            }
        }

        PwCommand::CreateCaptureInput { input_id, description, capture_device_name } => {
            create_input_sink(state, event_sender, core, input_id, &description);
            queue_input_to_filter_links(state, &[input_id]);
            queue_capture_links(state, input_id, &capture_device_name);
            try_resolve_pending_links(state);
        }

        PwCommand::BindCaptureToInput { input_id, capture_device_name } => {
            queue_capture_links(state, input_id, &capture_device_name);
            try_resolve_pending_links(state);
        }

        PwCommand::DestroyCaptureLoopback { input_id } => {
            let mut s = state.borrow_mut();
            remove_link_group(&mut s, &format!("capture:{input_id}"));
            s.capture_device_names.remove(&input_id);
        }

        PwCommand::SetCaptureVolume { input_id, pw_volume } => {
            let s = state.borrow();
            if let Some(ns) = s.input_sinks.get(&input_id) {
                set_node_channel_volumes(&ns._proxy, pw_volume);
            }
        }

        PwCommand::EnableLevelMonitoring => {
            let mut s = state.borrow_mut();
            if s.level_monitoring { return; }
            s.level_monitoring = true;
            info!("level monitoring enabled");
            let input_ids: Vec<u32> = s.input_sinks.keys().cloned().collect();
            drop(s);
            for input_id in input_ids {
                attach_level_listener(state, event_sender, input_id);
            }
        }

        PwCommand::DisableLevelMonitoring => {
            let mut s = state.borrow_mut();
            if !s.level_monitoring { return; }
            s.level_monitoring = false;
            s.level_listeners.clear();
            s.level_peaks.clear();
            info!("level monitoring disabled");
        }

        // -- DSP commands --

        PwCommand::SetInputEqEnabled { input_id, enabled } => {
            let s = state.borrow();
            if let Some(&iidx) = s.input_id_to_idx.get(&input_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_input_eq_enabled(iidx, enabled);
                }
            }
        }

        PwCommand::SetInputEqBand { input_id, band, band_type, freq, gain_db, q } => {
            let s = state.borrow();
            if let Some(&iidx) = s.input_id_to_idx.get(&input_id) {
                if let Some(ref mixer) = s.mixer {
                    let bt = match band_type.as_str() {
                        "low_shelf" => super::dsp::EqBandType::LowShelf,
                        "high_shelf" => super::dsp::EqBandType::HighShelf,
                        "bypass" => super::dsp::EqBandType::Bypass,
                        _ => super::dsp::EqBandType::Peaking,
                    };
                    mixer.set_input_eq_band(iidx, band as usize, bt, freq as f32, gain_db as f32, q as f32, 48000.0);
                }
            }
        }

        PwCommand::ResetInputEq { input_id } => {
            let s = state.borrow();
            if let Some(&iidx) = s.input_id_to_idx.get(&input_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.reset_input_eq(iidx);
                }
            }
        }

        PwCommand::SetInputGateEnabled { input_id, enabled } => {
            let s = state.borrow();
            if let Some(&iidx) = s.input_id_to_idx.get(&input_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_input_gate_enabled(iidx, enabled);
                }
            }
        }

        PwCommand::SetInputGate { input_id, threshold_db, attack_ms, release_ms, hold_ms } => {
            let s = state.borrow();
            if let Some(&iidx) = s.input_id_to_idx.get(&input_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_input_gate(iidx, threshold_db, attack_ms, release_ms, hold_ms, 48000.0);
                }
            }
        }

        PwCommand::SetInputDeesserEnabled { input_id, enabled } => {
            let s = state.borrow();
            if let Some(&iidx) = s.input_id_to_idx.get(&input_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_input_deesser_enabled(iidx, enabled);
                }
            }
        }

        PwCommand::SetInputDeesser { input_id, frequency, threshold_db, ratio } => {
            let s = state.borrow();
            if let Some(&iidx) = s.input_id_to_idx.get(&input_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_input_deesser(iidx, frequency as f32, threshold_db, ratio, 48000.0);
                }
            }
        }

        PwCommand::SetOutputCompressorEnabled { output_id, enabled } => {
            let s = state.borrow();
            if let Some(&oidx) = s.output_id_to_idx.get(&output_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_output_compressor_enabled(oidx, enabled);
                }
            }
        }

        PwCommand::SetOutputCompressor { output_id, threshold_db, ratio, attack_ms, release_ms, makeup_gain_db, knee_db } => {
            let s = state.borrow();
            if let Some(&oidx) = s.output_id_to_idx.get(&output_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_output_compressor(oidx, threshold_db, ratio, attack_ms, release_ms, makeup_gain_db, knee_db, 48000.0);
                }
            }
        }

        PwCommand::SetOutputLimiterEnabled { output_id, enabled } => {
            let s = state.borrow();
            if let Some(&oidx) = s.output_id_to_idx.get(&output_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_output_limiter_enabled(oidx, enabled);
                }
            }
        }

        PwCommand::SetOutputLimiter { output_id, ceiling_db, release_ms } => {
            let s = state.borrow();
            if let Some(&oidx) = s.output_id_to_idx.get(&output_id) {
                if let Some(ref mixer) = s.mixer {
                    mixer.set_output_limiter(oidx, ceiling_db, release_ms, 48000.0);
                }
            }
        }

        PwCommand::Shutdown { original_default_sink, original_stream_targets } => {
            cleanup_before_shutdown(state, original_default_sink, &original_stream_targets);
            state.borrow_mut().shutdown = true;
            main_loop.quit();
        }
    }
}

// ---------------------------------------------------------------------------
// Cleanup
// ---------------------------------------------------------------------------

fn cleanup_before_shutdown(
    state: &Rc<RefCell<PwState>>,
    original_default_sink: Option<String>,
    original_stream_targets: &HashMap<u32, String>,
) {
    let mut s = state.borrow_mut();

    // Step 1: Restore original stream targets via metadata.
    // This tells WirePlumber to move streams back to their original sinks
    // (e.g. Spotify back to the SDAC directly).
    if let Some(metadata) = &s.metadata {
        for &stream_id in &s.known_stream_ids {
            let original = original_stream_targets.get(&stream_id);
            metadata.set_property(stream_id, "target.node", Some("Spa:String:JSON"), original.map(|s| s.as_str()));
        }
    }

    // Step 2: Remove stream links first (stream → input sinks).
    // This releases streams so WirePlumber can reconnect them to original sinks.
    s.active_links.retain(|k, _| !k.starts_with("stream:"));
    s.pending_links.retain(|pl| !pl.group.starts_with("stream:"));

    // Step 3: Restore the default sink AFTER streams have been released.
    // WirePlumber will route them to the restored default.
    if let Some(metadata) = &s.metadata {
        metadata.set_property(0, "default.audio.sink", Some("Spa:String:JSON"), original_default_sink.as_deref());
    }

    // Step 4: Tear down the mixer chain from inside out:
    // input→mixer links, then mixer→output links, then mixer itself.
    s.active_links.retain(|k, _| k.starts_with("output_target:"));
    s.pending_links.retain(|pl| pl.group.starts_with("output_target:"));
    s.mixer = None;
    s.input_sinks.clear();

    // Step 5: Remove output→hardware links last.
    // The hardware sink (SDAC) should already have streams routed directly
    // to it by now, so removing these links doesn't interrupt its audio.
    s.active_links.clear();
    s.pending_links.clear();
    s.output_sources.clear();
    s.core = None;
    info!("PipeWire cleanup complete");
}

// ---------------------------------------------------------------------------
// Sink / source creation
// ---------------------------------------------------------------------------

fn create_input_sink(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    core: &CoreRc,
    input_id: u32,
    description: &str,
) {
    let node_name = format!("mixctl.input.{input_id}");
    let props = properties! {
        "factory.name" => "support.null-audio-sink",
        "node.name" => node_name.clone(),
        "node.description" => description.to_string(),
        "media.class" => "Audio/Sink",
        "audio.position" => "FL,FR,FC,LFE,RL,RR,SL,SR",
        "monitor.channel-volumes" => "true",
        "node.autoconnect" => "false",
        "object.linger" => "false",
    };

    match core.create_object::<Node>("adapter", &props) {
        Ok(proxy) => {
            let ev = event_sender.clone();
            let fired = Rc::new(Cell::new(false));
            let info_state = state.clone();
            let listener = proxy
                .add_listener_local()
                .info({
                    let fired = fired.clone();
                    move |info| {
                        if fired.get() { return; }
                        fired.set(true);
                        info_state.borrow_mut().input_sink_pw_ids.insert(input_id, info.id());
                        ev.send(PwEvent::InputSinkCreated { input_id, pw_node_id: info.id() }).ok();
                    }
                })
                .register();

            {
                let mut s = state.borrow_mut();
                if let Some(ref mut mixer) = s.mixer {
                    let idx = mixer.add_input_ports(input_id);
                    s.input_id_to_idx.insert(input_id, idx);
                }
            }

            state.borrow_mut().input_sinks.insert(input_id, PwNodeState {
                _proxy: proxy,
                _listener: Box::new(listener),
            });

            if state.borrow().level_monitoring {
                attach_level_listener(state, event_sender, input_id);
            }
        }
        Err(e) => {
            error!("failed to create input sink {node_name}: {e}");
            event_sender.send(PwEvent::Error { message: format!("failed to create input sink: {e}") }).ok();
        }
    }
}

fn create_output_source(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    core: &CoreRc,
    output_id: u32,
    description: &str,
) {
    let node_name = format!("mixctl.output.{output_id}");
    let props = properties! {
        "factory.name" => "support.null-audio-sink",
        "node.name" => node_name.clone(),
        "node.description" => description.to_string(),
        "media.class" => "Audio/Sink",
        "audio.position" => "FL,FR,FC,LFE,RL,RR,SL,SR",
        "monitor.channel-volumes" => "true",
        "node.autoconnect" => "false",
        "object.linger" => "false",
    };

    match core.create_object::<Node>("adapter", &props) {
        Ok(proxy) => {
            let ev = event_sender.clone();
            let fired = Rc::new(Cell::new(false));
            let listener = proxy
                .add_listener_local()
                .info({
                    let fired = fired.clone();
                    move |info| {
                        if fired.get() { return; }
                        fired.set(true);
                        ev.send(PwEvent::OutputSourceCreated { output_id, pw_node_id: info.id() }).ok();
                    }
                })
                .register();

            {
                let mut s = state.borrow_mut();
                if let Some(ref mut mixer) = s.mixer {
                    let idx = mixer.add_output_ports(output_id);
                    s.output_id_to_idx.insert(output_id, idx);
                }
            }

            state.borrow_mut().output_sources.insert(output_id, PwNodeState {
                _proxy: proxy,
                _listener: Box::new(listener),
            });
        }
        Err(e) => {
            error!("failed to create output source {node_name}: {e}");
            event_sender.send(PwEvent::Error { message: format!("failed to create output source: {e}") }).ok();
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn set_default_input(state: &Rc<RefCell<PwState>>, input_id: u32) {
    let s = state.borrow();
    if let Some(metadata) = &s.metadata {
        let target = format!("mixctl.input.{input_id}");
        let value = format!("{{\"name\": \"{target}\"}}");
        metadata.set_property(0, "default.audio.sink", Some("Spa:String:JSON"), Some(&value));
    } else {
        drop(s);
        state.borrow_mut().deferred_default_input = Some(input_id);
    }
}

fn attach_level_listener(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    input_id: u32,
) {
    let s = state.borrow();
    let node = match s.input_sinks.get(&input_id) {
        Some(ns) => &ns._proxy,
        None => return,
    };
    node.subscribe_params(&[ParamType::Props]);
    let level_state = state.clone();
    let level_ev = event_sender.clone();
    let listener = node
        .add_listener_local()
        .param(move |_seq, _param_id, _index, _next, param| {
            if let Some(pod) = param {
                if let Ok(value) = pipewire::spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes()) {
                    if let Value::Object(obj) = value.1 {
                        for prop in &obj.properties {
                            if prop.key == SPA_PROP_MONITOR_VOLUMES {
                                if let Value::ValueArray(ValueArray::Float(volumes)) = &prop.value {
                                    let peak = volumes.iter().copied().map(f32::abs).fold(0.0f32, f32::max);
                                    let mut ls = level_state.borrow_mut();
                                    ls.level_peaks.insert(input_id, peak);
                                    let now = Instant::now();
                                    if now.duration_since(ls.level_last_sent).as_millis() >= LEVEL_THROTTLE_MS {
                                        ls.level_last_sent = now;
                                        let levels: Vec<(u32, f32)> = ls.level_peaks.iter().map(|(&id, &lvl)| (id, lvl)).collect();
                                        drop(ls);
                                        level_ev.send(PwEvent::LevelUpdate { levels }).ok();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
        .register();
    drop(s);
    state.borrow_mut().level_listeners.insert(input_id, Box::new(listener));
}

fn set_node_channel_volumes(node: &Node, pw_volume: f32) {
    let volumes = vec![pw_volume; 8];
    let object = Object {
        type_: SpaTypes::ObjectParamProps.as_raw(),
        id: ParamType::Props.as_raw(),
        properties: vec![Property::new(
            SPA_PROP_CHANNEL_VOLUMES,
            Value::ValueArray(ValueArray::Float(volumes)),
        )],
    };
    let value = Value::Object(object);
    let mut buf = Vec::<u8>::new();
    match PodSerializer::serialize(Cursor::new(&mut buf), &value) {
        Ok(_) => {
            if let Some(pod) = Pod::from_bytes(&buf) {
                node.set_param(ParamType::Props, 0, pod);
            }
        }
        Err(e) => warn!("failed to serialize volume pod: {e:?}"),
    }
}
