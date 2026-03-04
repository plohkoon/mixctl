use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use pipewire as pw;
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

/// SPA_PROP_channelVolumes (0x20005 from spa/param/props.h)
const SPA_PROP_CHANNEL_VOLUMES: u32 = 0x20005;

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
    /// Spawn a PipeWire engine on a dedicated OS thread with automatic reconnection.
    ///
    /// On each connection (or reconnection), the PW thread creates a new
    /// `pipewire::channel::channel()` and sends `PwEvent::ChannelReady` with the sender.
    /// The tokio relay task swaps in the new sender to resume forwarding commands.
    ///
    /// The `shutdown_flag` is shared with the tokio side; set it before dropping the
    /// event channel to stop the retry loop.
    pub fn spawn(
        config: PwEngineConfig,
        shutdown_flag: Arc<AtomicBool>,
        event_sender: tokio::sync::mpsc::UnboundedSender<PwEvent>,
    ) -> Self {
        let thread_handle = thread::spawn(move || {
            pw::init();

            let mut backoff = Duration::from_secs(1);

            loop {
                // Create a fresh channel for each connection attempt
                let (new_tx, new_rx) = pipewire::channel::channel();
                event_sender
                    .send(PwEvent::ChannelReady { sender: new_tx })
                    .ok();

                run_pw_loop(&config, new_rx, &event_sender);

                if shutdown_flag.load(Ordering::Relaxed) {
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

    let state = Rc::new(RefCell::new(PwState {
        input_sinks: HashMap::new(),
        output_sources: HashMap::new(),
        route_loopbacks: HashMap::new(),
        output_target_loopbacks: HashMap::new(),
        capture_loopbacks: HashMap::new(),
        route_playback_nodes: HashMap::new(),
        route_playback_node_ids: HashMap::new(),
        metadata: None,
        _metadata_listener: None,
        deferred_default_input: config.default_input_id,
        known_stream_ids: HashSet::new(),
        known_capture_ids: HashSet::new(),
        shutdown: false,
    }));

    // Listen for registry events
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
                // Clean up route playback node tracking
                if let Some(key) = s.route_playback_node_ids.remove(&id) {
                    s.route_playback_nodes.remove(&key);
                } else if s.known_stream_ids.remove(&id) {
                    ev.send(PwEvent::StreamRemoved { pw_node_id: id }).ok();
                } else if s.known_capture_ids.remove(&id) {
                    ev.send(PwEvent::CaptureDeviceRemoved { pw_node_id: id }).ok();
                }
            }
        })
        .register();

    // Listen for core errors
    let error_event_sender = event_sender.clone();
    let _core_listener = core
        .add_listener_local()
        .error(move |_id, _seq, _res, msg| {
            error!("PipeWire core error: {msg}");
            error_event_sender.send(PwEvent::Error {
                message: msg.to_string(),
            }).ok();
        })
        .register();

    // Attach command receiver to the main loop
    let cmd_state = state.clone();
    let cmd_event_sender = event_sender.clone();
    let cmd_main_loop = main_loop.clone();
    let cmd_context = context.clone();
    let cmd_core = core.clone();
    // Keep-alive: dropping unregisters the command handler from the main loop
    let _cmd_receiver = cmd_receiver.attach(main_loop.loop_(), move |cmd| {
        handle_command(
            &cmd_state,
            &cmd_event_sender,
            &cmd_main_loop,
            &cmd_context,
            &cmd_core,
            cmd,
        );
    });

    // Signal successful connection
    event_sender.send(PwEvent::Connected).ok();
    info!("PipeWire connected");

    // Create initial input sinks
    for cfg in &config.inputs {
        create_input_sink(&state, &event_sender, &core, cfg.input_id, &cfg.description);
    }

    // Create initial output sources
    for cfg in &config.outputs {
        create_output_source(&state, &event_sender, &core, cfg.output_id, &cfg.description);
    }

    // Create initial route loopbacks (targeting output sources directly)
    for cfg in &config.routes {
        create_route_loopback(&state, &event_sender, &context, cfg.input_id, cfg.output_id, cfg.pw_volume);
    }

    // Restore output target loopbacks from config
    for cfg in &config.output_targets {
        let source_name = format!("mixctl.output.{}", cfg.output_id);
        create_output_target_loopback(&state, &context, cfg.output_id, &source_name, &cfg.device_name);
    }

    // Restore capture input loopbacks from config
    for cfg in &config.capture_inputs {
        let input_sink_name = format!("mixctl.input.{}", cfg.input_id);
        create_capture_loopback(&state, &context, cfg.input_id, &cfg.capture_device_name, &input_sink_name, 1.0);
    }

    // Run the main loop — blocks until quit() is called
    main_loop.run();

    if !state.borrow().shutdown {
        event_sender.send(PwEvent::Disconnected).ok();
    }
    info!("PipeWire main loop exited");
}

/// Internal state for the PipeWire thread. All PW objects live here (not Send/Sync).
struct PwState {
    input_sinks: HashMap<u32, PwNodeState>,
    output_sources: HashMap<u32, PwNodeState>,
    route_loopbacks: HashMap<(u32, u32), PwLoopbackState>,
    output_target_loopbacks: HashMap<u32, PwLoopbackState>,
    capture_loopbacks: HashMap<u32, PwCaptureState>,
    /// Playback-side Node proxies for route loopbacks, used for in-place volume updates.
    /// Populated when the loopback's playback node appears in the registry.
    route_playback_nodes: HashMap<(u32, u32), Node>,
    /// Reverse map: pw_node_id → (input_id, output_id) for cleanup on global_remove.
    route_playback_node_ids: HashMap<u32, (u32, u32)>,
    metadata: Option<pw::metadata::Metadata>,
    _metadata_listener: Option<Box<dyn std::any::Any>>,
    deferred_default_input: Option<u32>,
    known_stream_ids: HashSet<u32>,
    known_capture_ids: HashSet<u32>,
    shutdown: bool,
}

struct PwNodeState {
    _proxy: Node,
    _listener: Box<dyn std::any::Any>,
}

struct PwLoopbackState {
    _module_ptr: *mut pw::sys::pw_impl_module,
}

struct PwCaptureState {
    _module_ptr: *mut pw::sys::pw_impl_module,
    device_name: String,
    sink_name: String,
}

// pw_impl_module is thread-local; this is safe because PwLoopbackState
// is only used within the single PW thread.
unsafe impl Send for PwLoopbackState {}
unsafe impl Send for PwCaptureState {}

fn handle_registry_global(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    registry: &pw::registry::Registry,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
) {
    match global.type_ {
        ObjectType::Metadata => handle_metadata_global(state, registry, global),
        ObjectType::Node => handle_node_global(state, event_sender, registry, global),
        _ => {}
    }
}

fn handle_metadata_global(
    state: &Rc<RefCell<PwState>>,
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
        Err(e) => {
            warn!("failed to bind metadata: {e}");
            return;
        }
    };

    debug!("bound default metadata object (id={})", global.id);

    // If we had a deferred default input, set it now
    if let Some(input_id) = s.deferred_default_input.take() {
        let target = format!("mixctl.input.{input_id}");
        let value = format!("{{\"name\": \"{target}\"}}");
        metadata.set_property(
            0,
            "default.audio.sink",
            Some("Spa:String:JSON"),
            Some(&value),
        );
        debug!("set deferred default audio sink to input {input_id}");
    }

    s.metadata = Some(metadata);
}

fn handle_node_global(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    registry: &pw::registry::Registry,
    global: &pw::registry::GlobalObject<&pw::spa::utils::dict::DictRef>,
) {
    let props = match &global.props {
        Some(p) => p,
        None => return,
    };

    let media_class = props.get("media.class").unwrap_or("");

    match media_class {
        "Stream/Output/Audio" => {
            let node_name = props.get("node.name").unwrap_or("");

            // Track route loopback playback nodes for in-place volume updates (D2)
            if let Some(route_suffix) = node_name.strip_prefix("mixctl.route.") {
                if let Some((iid, oid)) = parse_route_ids(route_suffix) {
                    match registry.bind::<Node, _>(global) {
                        Ok(node) => {
                            let mut s = state.borrow_mut();
                            s.route_playback_nodes.insert((iid, oid), node);
                            s.route_playback_node_ids.insert(global.id, (iid, oid));
                            debug!("tracked route playback node: {node_name} (pw_id={})", global.id);
                        }
                        Err(e) => {
                            warn!("failed to bind route playback node {node_name}: {e}");
                        }
                    }
                }
                return;
            }

            // Filter out all other mixctl.* nodes from stream detection (M4)
            if node_name.starts_with("mixctl.") {
                return;
            }

            let app_name = props
                .get("application.name")
                .or_else(|| props.get("node.name"))
                .unwrap_or("unknown")
                .to_string();
            let media_name = props.get("media.name").unwrap_or("").to_string();
            state.borrow_mut().known_stream_ids.insert(global.id);
            event_sender
                .send(PwEvent::StreamAppeared {
                    pw_node_id: global.id,
                    app_name,
                    media_name,
                })
                .ok();
        }
        "Audio/Source" => {
            let node_name = props.get("node.name").unwrap_or("");
            if node_name.starts_with("mixctl.") {
                return;
            }
            let name = props
                .get("node.description")
                .or_else(|| props.get("node.nick"))
                .unwrap_or(node_name)
                .to_string();
            state.borrow_mut().known_capture_ids.insert(global.id);
            event_sender
                .send(PwEvent::CaptureDeviceAppeared {
                    pw_node_id: global.id,
                    name,
                    device_name: node_name.to_string(),
                })
                .ok();
        }
        _ => {}
    }
}

/// Parse "input_id.output_id" from a route node name suffix.
fn parse_route_ids(suffix: &str) -> Option<(u32, u32)> {
    let mut parts = suffix.splitn(2, '.');
    let iid = parts.next()?.parse::<u32>().ok()?;
    let oid = parts.next()?.parse::<u32>().ok()?;
    Some((iid, oid))
}

fn handle_command(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    main_loop: &pw::main_loop::MainLoop,
    context: &pw::context::Context,
    core: &pw::core::Core,
    cmd: PwCommand,
) {
    match cmd {
        PwCommand::CreateInputSink {
            input_id,
            description,
        } => {
            create_input_sink(state, event_sender, core, input_id, &description);
        }

        PwCommand::DestroyInputSink { input_id } => {
            let mut s = state.borrow_mut();
            // Destroy route loopbacks for this input
            let route_keys: Vec<(u32, u32)> = s
                .route_loopbacks
                .keys()
                .filter(|(iid, _)| *iid == input_id)
                .cloned()
                .collect();
            for key in route_keys {
                if let Some(lb) = s.route_loopbacks.remove(&key) {
                    destroy_module(lb._module_ptr);
                }
            }
            // Destroy capture loopback
            if let Some(lb) = s.capture_loopbacks.remove(&input_id) {
                destroy_module(lb._module_ptr);
            }
            // Destroy input sink
            if s.input_sinks.remove(&input_id).is_some() {
                event_sender
                    .send(PwEvent::InputSinkDestroyed { input_id })
                    .ok();
                debug!("destroyed input sink for input {input_id}");
            }
        }

        PwCommand::SetDefaultInput { input_id } => {
            set_default_input(state, input_id);
        }

        PwCommand::RenameInputSink {
            input_id,
            description,
        } => {
            let mut s = state.borrow_mut();
            let existed = s.input_sinks.remove(&input_id).is_some();
            drop(s);
            if existed {
                create_input_sink(state, event_sender, core, input_id, &description);
            }
        }

        PwCommand::CreateOutputSource {
            output_id,
            description,
        } => {
            create_output_source(state, event_sender, core, output_id, &description);
        }

        PwCommand::DestroyOutputSource { output_id } => {
            let mut s = state.borrow_mut();
            // Destroy route loopbacks first
            let route_keys: Vec<(u32, u32)> = s
                .route_loopbacks
                .keys()
                .filter(|(_, oid)| *oid == output_id)
                .cloned()
                .collect();
            for key in route_keys {
                if let Some(lb) = s.route_loopbacks.remove(&key) {
                    destroy_module(lb._module_ptr);
                }
            }
            // Destroy output target loopback
            if let Some(lb) = s.output_target_loopbacks.remove(&output_id) {
                destroy_module(lb._module_ptr);
            }
            // Destroy output source
            if s.output_sources.remove(&output_id).is_some() {
                event_sender
                    .send(PwEvent::OutputSourceDestroyed { output_id })
                    .ok();
                debug!("destroyed output source for output {output_id}");
            }
        }

        PwCommand::RenameOutputSource {
            output_id,
            description,
        } => {
            let mut s = state.borrow_mut();
            let existed = s.output_sources.remove(&output_id).is_some();
            drop(s);
            if existed {
                create_output_source(state, event_sender, core, output_id, &description);
            }
        }

        PwCommand::SetRouteLink {
            input_id,
            output_id,
            volume,
        } => {
            let s = state.borrow();
            let has_loopback = s.route_loopbacks.contains_key(&(input_id, output_id));
            let has_playback_node = s.route_playback_nodes.contains_key(&(input_id, output_id));
            drop(s);

            if has_loopback && has_playback_node {
                // In-place volume update via set_param (avoids audio glitch)
                let s = state.borrow();
                if let Some(node) = s.route_playback_nodes.get(&(input_id, output_id)) {
                    set_node_channel_volumes(node, volume);
                    debug!("in-place volume update: {input_id} → {output_id} (pw_volume={volume})");
                }
            } else {
                // Fallback: destroy and recreate the loopback module
                let mut s = state.borrow_mut();
                if let Some(lb) = s.route_loopbacks.remove(&(input_id, output_id)) {
                    destroy_module(lb._module_ptr);
                }
                drop(s);
                create_route_loopback(state, event_sender, context, input_id, output_id, volume);
            }
        }

        PwCommand::DestroyRouteLink {
            input_id,
            output_id,
        } => {
            let mut s = state.borrow_mut();
            if let Some(lb) = s.route_loopbacks.remove(&(input_id, output_id)) {
                destroy_module(lb._module_ptr);
                debug!("destroyed route loopback {input_id} → {output_id}");
            }
        }

        PwCommand::SetOutputTarget {
            output_id,
            device_name,
        } => {
            {
                let mut s = state.borrow_mut();
                if let Some(lb) = s.output_target_loopbacks.remove(&output_id) {
                    destroy_module(lb._module_ptr);
                }
            }
            if let Some(device) = device_name {
                let source_name = format!("mixctl.output.{output_id}");
                create_output_target_loopback(state, context, output_id, &source_name, &device);
            }
        }

        PwCommand::MoveStream {
            pw_node_id,
            input_id,
        } => {
            let s = state.borrow();
            if let Some(metadata) = &s.metadata {
                let target = format!("mixctl.input.{input_id}");
                let value = format!("{{\"name\": \"{target}\"}}");
                metadata.set_property(
                    pw_node_id,
                    "target.node",
                    Some("Spa:String:JSON"),
                    Some(&value),
                );
                debug!("moved stream {pw_node_id} to input {input_id}");
            } else {
                warn!("cannot move stream: metadata not available");
            }
        }

        PwCommand::CreateCaptureInput {
            input_id,
            description,
            capture_device_name,
        } => {
            create_input_sink(state, event_sender, core, input_id, &description);
            let input_sink_name = format!("mixctl.input.{input_id}");
            create_capture_loopback(state, context, input_id, &capture_device_name, &input_sink_name, 1.0);
        }

        PwCommand::DestroyCaptureLoopback { input_id } => {
            let mut s = state.borrow_mut();
            if let Some(lb) = s.capture_loopbacks.remove(&input_id) {
                destroy_module(lb._module_ptr);
                debug!("destroyed capture loopback for input {input_id}");
            }
        }

        PwCommand::SetCaptureVolume { input_id, pw_volume } => {
            let mut s = state.borrow_mut();
            if let Some(cap) = s.capture_loopbacks.remove(&input_id) {
                let device_name = cap.device_name.clone();
                let sink_name = cap.sink_name.clone();
                destroy_module(cap._module_ptr);
                drop(s);
                create_capture_loopback(state, context, input_id, &device_name, &sink_name, pw_volume);
            }
        }

        PwCommand::Shutdown => {
            state.borrow_mut().shutdown = true;
            main_loop.quit();
        }
    }
}

fn create_input_sink(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    core: &pw::core::Core,
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
            let listener = proxy
                .add_listener_local()
                .info({
                    let fired = fired.clone();
                    move |info| {
                        if fired.get() {
                            return;
                        }
                        fired.set(true);
                        ev.send(PwEvent::InputSinkCreated {
                            input_id,
                            pw_node_id: info.id(),
                        })
                        .ok();
                    }
                })
                .register();

            state.borrow_mut().input_sinks.insert(
                input_id,
                PwNodeState {
                    _proxy: proxy,
                    _listener: Box::new(listener),
                },
            );
            debug!("created input sink: {node_name} ({description})");
        }
        Err(e) => {
            error!("failed to create input sink {node_name}: {e}");
            event_sender
                .send(PwEvent::Error {
                    message: format!("failed to create input sink: {e}"),
                })
                .ok();
        }
    }
}

fn create_output_source(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    core: &pw::core::Core,
    output_id: u32,
    description: &str,
) {
    let node_name = format!("mixctl.output.{output_id}");
    // null-audio-sink with Audio/Source/Virtual creates a source node. The Virtual
    // suffix ensures it passes through the registry's Audio/Source detection for
    // capture devices, while the mixctl. name prefix filters it out.
    let props = properties! {
        "factory.name" => "support.null-audio-sink",
        "node.name" => node_name.clone(),
        "node.description" => description.to_string(),
        "media.class" => "Audio/Source/Virtual",
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
                        if fired.get() {
                            return;
                        }
                        fired.set(true);
                        ev.send(PwEvent::OutputSourceCreated {
                            output_id,
                            pw_node_id: info.id(),
                        })
                        .ok();
                    }
                })
                .register();

            state.borrow_mut().output_sources.insert(
                output_id,
                PwNodeState {
                    _proxy: proxy,
                    _listener: Box::new(listener),
                },
            );
            debug!("created output source: {node_name} ({description})");
        }
        Err(e) => {
            error!("failed to create output source {node_name}: {e}");
            event_sender
                .send(PwEvent::Error {
                    message: format!("failed to create output source: {e}"),
                })
                .ok();
        }
    }
}

fn load_module_raw(
    context: &pw::context::Context,
    module_name: &str,
    args: &str,
) -> *mut pw::sys::pw_impl_module {
    let name_c = CString::new(module_name).expect("null byte in module name");
    let args_c = CString::new(args).expect("null byte in module args");
    unsafe {
        pw::sys::pw_context_load_module(
            context.as_raw_ptr(),
            name_c.as_ptr(),
            args_c.as_ptr(),
            std::ptr::null_mut(),
        )
    }
}

fn destroy_module(module_ptr: *mut pw::sys::pw_impl_module) {
    if !module_ptr.is_null() {
        unsafe {
            pw::sys::pw_impl_module_destroy(module_ptr);
        }
    }
}

fn create_route_loopback(
    state: &Rc<RefCell<PwState>>,
    event_sender: &tokio::sync::mpsc::UnboundedSender<PwEvent>,
    context: &pw::context::Context,
    input_id: u32,
    output_id: u32,
    pw_volume: f32,
) {
    let input_target = format!("mixctl.input.{input_id}");
    let output_target = format!("mixctl.output.{output_id}");
    let node_name = format!("mixctl.route.{input_id}.{output_id}");

    let args = format!(
        "{{ \
            node.name = {node_name} \
            node.description = \"Route {input_id} to {output_id}\" \
            capture.props = {{ \
                target.object = {input_target} \
                stream.capture.sink = true \
                audio.position = FL,FR,FC,LFE,RL,RR,SL,SR \
            }} \
            playback.props = {{ \
                target.object = {output_target} \
                audio.position = FL,FR,FC,LFE,RL,RR,SL,SR \
                channelmix.normalize = false \
                channelmix.volume = {pw_volume} \
            }} \
        }}"
    );

    let module_ptr = load_module_raw(context, "libpipewire-module-loopback", &args);

    if module_ptr.is_null() {
        error!("failed to create route loopback {input_id} → {output_id}");
        event_sender
            .send(PwEvent::Error {
                message: format!("failed to create route loopback {input_id} → {output_id}"),
            })
            .ok();
        return;
    }

    state.borrow_mut().route_loopbacks.insert(
        (input_id, output_id),
        PwLoopbackState {
            _module_ptr: module_ptr,
        },
    );

    event_sender
        .send(PwEvent::RouteLinkCreated {
            input_id,
            output_id,
        })
        .ok();
    debug!("created route loopback: {input_id} → {output_id} (pw_volume={pw_volume})");
}

fn set_default_input(state: &Rc<RefCell<PwState>>, input_id: u32) {
    let s = state.borrow();
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
    } else {
        debug!("metadata not yet available, deferring default input {input_id}");
        drop(s);
        state.borrow_mut().deferred_default_input = Some(input_id);
    }
}

fn create_output_target_loopback(
    state: &Rc<RefCell<PwState>>,
    context: &pw::context::Context,
    output_id: u32,
    source_name: &str,
    device_name: &str,
) {
    let node_name = format!("mixctl.output-target.{output_id}");

    let args = format!(
        "{{ \
            node.name = {node_name} \
            node.description = \"Output {output_id} to Device\" \
            capture.props = {{ \
                target.object = {source_name} \
                stream.capture.sink = true \
                audio.position = FL,FR,FC,LFE,RL,RR,SL,SR \
            }} \
            playback.props = {{ \
                target.object = {device_name} \
                audio.position = FL,FR,FC,LFE,RL,RR,SL,SR \
            }} \
        }}"
    );

    let module_ptr = load_module_raw(context, "libpipewire-module-loopback", &args);

    if module_ptr.is_null() {
        error!("failed to create output target loopback for output {output_id}");
        return;
    }

    state.borrow_mut().output_target_loopbacks.insert(
        output_id,
        PwLoopbackState {
            _module_ptr: module_ptr,
        },
    );
    debug!("created output target loopback: {source_name} → {device_name}");
}

fn create_capture_loopback(
    state: &Rc<RefCell<PwState>>,
    context: &pw::context::Context,
    input_id: u32,
    capture_device_name: &str,
    input_sink_name: &str,
    pw_volume: f32,
) {
    let node_name = format!("mixctl.capture.{input_id}");

    let args = format!(
        "{{ \
            node.name = {node_name} \
            node.description = \"Capture to Input {input_id}\" \
            capture.props = {{ \
                target.object = {capture_device_name} \
                audio.position = FL,FR,FC,LFE,RL,RR,SL,SR \
            }} \
            playback.props = {{ \
                target.object = {input_sink_name} \
                audio.position = FL,FR,FC,LFE,RL,RR,SL,SR \
                channelmix.normalize = false \
                channelmix.volume = {pw_volume} \
            }} \
        }}"
    );

    let module_ptr = load_module_raw(context, "libpipewire-module-loopback", &args);

    if module_ptr.is_null() {
        error!("failed to create capture loopback for input {input_id}");
        return;
    }

    state.borrow_mut().capture_loopbacks.insert(
        input_id,
        PwCaptureState {
            _module_ptr: module_ptr,
            device_name: capture_device_name.to_string(),
            sink_name: input_sink_name.to_string(),
        },
    );
    debug!("created capture loopback: {capture_device_name} → {input_sink_name}");
}

/// Update channel volumes on a node in-place via set_param, avoiding module destroy/recreate.
fn set_node_channel_volumes(node: &Node, pw_volume: f32) {
    // Build a Props object with channelVolumes = [vol × 8 channels]
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
            } else {
                warn!("failed to parse serialized volume pod");
            }
        }
        Err(e) => {
            warn!("failed to serialize volume pod: {e:?}");
        }
    }
}
