//! pw_filter-based mixer node.
//!
//! `VolumeMatrix` stores per-input-per-output volume scalars in a lock-free
//! grid of `AtomicU32` values (f32 bits), readable from the RT process callback.
//!
//! `MixerFilter` wraps the raw `pw_filter` FFI to create a DSP filter node
//! ("mixctl.mixer") with dynamically added input/output ports. The process
//! callback dequeues each port buffer exactly once per cycle, zeroes output
//! buffers, then mixes scaled input samples into outputs using cached pointers.

use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use pipewire as pw;
use tracing::{debug, error, warn};

/// 8 channels: FL, FR, FC, LFE, RL, RR, SL, SR
pub const NUM_CHANNELS: usize = 8;
pub const CHANNELS: [&str; NUM_CHANNELS] = ["FL", "FR", "FC", "LFE", "RL", "RR", "SL", "SR"];

/// Maximum number of inputs and outputs the volume matrix supports.
const MAX_SLOTS: usize = 16;

// ---------------------------------------------------------------------------
// VolumeMatrix
// ---------------------------------------------------------------------------

/// Lock-free volume matrix readable from the RT thread.
///
/// Layout: `data[input_idx * MAX_SLOTS + output_idx]` stores the f32 volume
/// as raw bits in an `AtomicU32`.
pub struct VolumeMatrix {
    data: Box<[AtomicU32]>,
}

// AtomicU32 is Send+Sync, but the Box<[AtomicU32]> needs explicit impls
// because we share it via Arc between the PW thread and the RT callback.
unsafe impl Send for VolumeMatrix {}
unsafe impl Sync for VolumeMatrix {}

impl VolumeMatrix {
    pub fn new() -> Self {
        let mut v = Vec::with_capacity(MAX_SLOTS * MAX_SLOTS);
        for _ in 0..MAX_SLOTS * MAX_SLOTS {
            v.push(AtomicU32::new(0));
        }
        Self { data: v.into_boxed_slice() }
    }

    /// Set volume for (input_idx, output_idx). Called from the PW main thread.
    pub fn set(&self, input_idx: usize, output_idx: usize, volume: f32) {
        if input_idx < MAX_SLOTS && output_idx < MAX_SLOTS {
            self.data[input_idx * MAX_SLOTS + output_idx]
                .store(volume.to_bits(), Ordering::Release);
        }
    }

    /// Get volume for (input_idx, output_idx). RT-safe (atomic load).
    #[inline]
    pub fn get(&self, input_idx: usize, output_idx: usize) -> f32 {
        if input_idx < MAX_SLOTS && output_idx < MAX_SLOTS {
            f32::from_bits(
                self.data[input_idx * MAX_SLOTS + output_idx].load(Ordering::Relaxed),
            )
        } else {
            0.0
        }
    }
}

// ---------------------------------------------------------------------------
// MixerFilter
// ---------------------------------------------------------------------------

/// Data shared with the RT process callback.
struct MixerCallbackData {
    volume_matrix: Arc<VolumeMatrix>,
    /// input_ports[input_idx][channel] = port_data pointer
    input_ports: Vec<[*mut c_void; NUM_CHANNELS]>,
    /// output_ports[output_idx][channel] = port_data pointer
    output_ports: Vec<[*mut c_void; NUM_CHANNELS]>,
}

// These pointers are only used within the single PW RT thread.
unsafe impl Send for MixerCallbackData {}

/// Wrapper around a raw `pw_filter` acting as the central mixer node.
pub struct MixerFilter {
    filter: *mut pw::sys::pw_filter,
    /// Leaked Box — freed when the filter is destroyed.
    callback_data: *mut MixerCallbackData,
    /// Heap-allocated listener hook — must NOT move after pw_filter_add_listener
    /// because PipeWire stores a pointer to it internally (spa_list linkage).
    _listener: Box<pw::spa::sys::spa_hook>,
    /// Heap-allocated events — must stay alive while listener is registered.
    _events: Box<pw::sys::pw_filter_events>,
    /// Number of input slots currently added.
    pub num_inputs: usize,
    /// Number of output slots currently added.
    pub num_outputs: usize,
}

// pw_filter is thread-local to the PW thread.
unsafe impl Send for MixerFilter {}

impl MixerFilter {
    /// Create a new mixer filter attached to `core`.
    ///
    /// The filter is NOT yet connected — call `connect()` after adding
    /// initial ports.
    pub fn new(core: &pw::core::Core, volume_matrix: Arc<VolumeMatrix>) -> Option<Self> {
        let name = CString::new("mixctl.mixer").unwrap();

        let filter = unsafe {
            let props = pw::sys::pw_properties_new(std::ptr::null::<std::os::raw::c_char>());
            macro_rules! set_prop {
                ($k:expr, $v:expr) => {{
                    let k = CString::new($k).unwrap();
                    let v = CString::new($v).unwrap();
                    pw::sys::pw_properties_set(props, k.as_ptr(), v.as_ptr());
                }};
            }
            set_prop!("media.type", "Audio");
            set_prop!("media.category", "Filter");
            set_prop!("media.role", "DSP");
            set_prop!("node.name", "mixctl.mixer");
            set_prop!("node.description", "MixCtl Mixer");
            set_prop!("node.autoconnect", "false");
            set_prop!("object.linger", "false");
            pw::sys::pw_filter_new(core.as_raw_ptr(), name.as_ptr(), props)
        };

        if filter.is_null() {
            error!("failed to create pw_filter for mixer");
            return None;
        }

        let callback_data = Box::into_raw(Box::new(MixerCallbackData {
            volume_matrix,
            input_ports: Vec::new(),
            output_ports: Vec::new(),
        }));

        // Set up events with process callback — heap-allocated so it stays at a stable address
        let events = Box::new(pw::sys::pw_filter_events {
            version: pw::sys::PW_VERSION_FILTER_EVENTS,
            destroy: None,
            state_changed: Some(mixer_state_changed),
            io_changed: None,
            param_changed: None,
            add_buffer: None,
            remove_buffer: None,
            process: Some(mixer_process),
            drained: None,
            command: None,
        });

        // Heap-allocate the hook so it stays at a stable address.
        // PipeWire stores a pointer to it internally (spa_list linkage).
        let (listener, events) = unsafe {
            let hook: Box<pw::spa::sys::spa_hook> = Box::new(std::mem::zeroed());
            let raw_hook = Box::into_raw(hook);
            let raw_events = Box::into_raw(events);
            pw::sys::pw_filter_add_listener(
                filter,
                raw_hook,
                raw_events,
                callback_data as *mut c_void,
            );
            (Box::from_raw(raw_hook), Box::from_raw(raw_events))
        };

        Some(MixerFilter {
            filter,
            callback_data,
            _listener: listener,
            _events: events,
            num_inputs: 0,
            num_outputs: 0,
        })
    }

    /// Add 8 input ports for a logical input (one per channel).
    /// Returns the input index (0-based) used in the volume matrix.
    pub fn add_input_ports(&mut self, input_id: u32) -> usize {
        let idx = self.num_inputs;
        let cb = unsafe { &mut *self.callback_data };
        let mut ports = [std::ptr::null_mut(); NUM_CHANNELS];

        for (ch_idx, ch_name) in CHANNELS.iter().enumerate() {
            let port_name_c = CString::new(format!("in_{input_id}_{ch_name}")).unwrap();
            let key_fmt = CString::new("format.dsp").unwrap();
            let val_fmt = CString::new("32 bit float mono audio").unwrap();
            let key_name = CString::new("port.name").unwrap();

            let port_data = unsafe {
                let props = pw::sys::pw_properties_new(
                    key_name.as_ptr(), port_name_c.as_ptr(),
                    key_fmt.as_ptr(), val_fmt.as_ptr(),
                    std::ptr::null::<std::os::raw::c_char>(),
                );
                pw::sys::pw_filter_add_port(
                    self.filter,
                    pw::spa::sys::SPA_DIRECTION_INPUT,
                    pw::sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
                    std::mem::size_of::<u8>(),
                    props,
                    std::ptr::null_mut(),
                    0,
                )
            };

            if port_data.is_null() {
                warn!("failed to add input port in_{input_id}_{ch_name}");
            }
            ports[ch_idx] = port_data;
        }

        cb.input_ports.push(ports);
        self.num_inputs += 1;
        debug!("mixer: added input ports for input {input_id} (idx={idx})");
        idx
    }

    /// Add 8 output ports for a logical output (one per channel).
    /// Returns the output index (0-based) used in the volume matrix.
    pub fn add_output_ports(&mut self, output_id: u32) -> usize {
        let idx = self.num_outputs;
        let cb = unsafe { &mut *self.callback_data };
        let mut ports = [std::ptr::null_mut(); NUM_CHANNELS];

        for (ch_idx, ch_name) in CHANNELS.iter().enumerate() {
            let port_name_c = CString::new(format!("out_{output_id}_{ch_name}")).unwrap();
            let key_fmt = CString::new("format.dsp").unwrap();
            let val_fmt = CString::new("32 bit float mono audio").unwrap();
            let key_name = CString::new("port.name").unwrap();

            let port_data = unsafe {
                let props = pw::sys::pw_properties_new(
                    key_name.as_ptr(), port_name_c.as_ptr(),
                    key_fmt.as_ptr(), val_fmt.as_ptr(),
                    std::ptr::null::<std::os::raw::c_char>(),
                );
                pw::sys::pw_filter_add_port(
                    self.filter,
                    pw::spa::sys::SPA_DIRECTION_OUTPUT,
                    pw::sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
                    std::mem::size_of::<u8>(),
                    props,
                    std::ptr::null_mut(),
                    0,
                )
            };

            if port_data.is_null() {
                warn!("failed to add output port out_{output_id}_{ch_name}");
            }
            ports[ch_idx] = port_data;
        }

        cb.output_ports.push(ports);
        self.num_outputs += 1;
        debug!("mixer: added output ports for output {output_id} (idx={idx})");
        idx
    }

    /// Remove 8 input ports for a logical input at the given index.
    pub fn remove_input_ports(&mut self, idx: usize) {
        let cb = unsafe { &mut *self.callback_data };
        if idx < cb.input_ports.len() {
            let ports = cb.input_ports.remove(idx);
            for port_data in ports {
                if !port_data.is_null() {
                    unsafe { pw::sys::pw_filter_remove_port(port_data); }
                }
            }
            self.num_inputs = self.num_inputs.saturating_sub(1);
        }
    }

    /// Remove 8 output ports for a logical output at the given index.
    pub fn remove_output_ports(&mut self, idx: usize) {
        let cb = unsafe { &mut *self.callback_data };
        if idx < cb.output_ports.len() {
            let ports = cb.output_ports.remove(idx);
            for port_data in ports {
                if !port_data.is_null() {
                    unsafe { pw::sys::pw_filter_remove_port(port_data); }
                }
            }
            self.num_outputs = self.num_outputs.saturating_sub(1);
        }
    }

    /// Connect the filter to the graph with RT processing enabled.
    pub fn connect(&self) -> bool {
        let ret = unsafe {
            pw::sys::pw_filter_connect(
                self.filter,
                pw::sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS,
                std::ptr::null_mut(),
                0,
            )
        };
        if ret < 0 {
            error!("pw_filter_connect failed: {ret}");
            false
        } else {
            debug!("mixer filter connected");
            true
        }
    }
}

impl Drop for MixerFilter {
    fn drop(&mut self) {
        if !self.filter.is_null() {
            unsafe {
                pw::sys::pw_filter_destroy(self.filter);
                // Reclaim the leaked callback data
                drop(Box::from_raw(self.callback_data));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FFI callbacks (called from the PW RT thread)
// ---------------------------------------------------------------------------

/// Dequeue a buffer from a port, set chunk metadata for output ports,
/// re-queue it, and return the data pointer. The pointer stays valid within
/// the current process cycle because `PW_FILTER_PORT_FLAG_MAP_BUFFERS` is set.
///
/// This mirrors `pw_filter_get_dsp_buffer` internally (dequeue → set chunk →
/// queue → return data pointer). Each port buffer can only be dequeued **once
/// per process cycle** — after queue, the buffer moves to the `queued` ring
/// and a second dequeue returns NULL.
///
/// # Safety
/// Must be called from the PW RT process callback. Call at most once per port
/// per cycle.
#[inline]
unsafe fn dequeue_raw_buffer(
    port_data: *mut c_void,
    n_samples: u32,
    is_output: bool,
) -> *mut f32 {
    unsafe {
        let pw_buf = pw::sys::pw_filter_dequeue_buffer(port_data);
        if pw_buf.is_null() {
            return std::ptr::null_mut();
        }
        let spa_buf = (*pw_buf).buffer;
        if (*spa_buf).n_datas == 0 || (*(*spa_buf).datas).data.is_null() {
            pw::sys::pw_filter_queue_buffer(port_data, pw_buf);
            return std::ptr::null_mut();
        }
        let d = &*(*spa_buf).datas;
        if is_output {
            (*d.chunk).offset = 0;
            (*d.chunk).size = n_samples * std::mem::size_of::<f32>() as u32;
            (*d.chunk).stride = std::mem::size_of::<f32>() as i32;
        }
        let data = d.data as *mut f32;
        pw::sys::pw_filter_queue_buffer(port_data, pw_buf);
        data
    }
}

/// Process callback: mix all inputs into all outputs with volume scaling.
///
/// Each port buffer is dequeued exactly once into a stack-local pointer array.
/// Output buffers are zeroed during dequeue, then the mix loop accumulates
/// scaled input samples using the cached pointers (no further dequeue calls).
///
/// # Safety
/// Called from the PW realtime thread. `data` must be a valid `MixerCallbackData` pointer.
unsafe extern "C" fn mixer_process(
    data: *mut c_void,
    position: *mut pw::spa::sys::spa_io_position,
) {
    unsafe {
        let ctx = &*(data as *const MixerCallbackData);
        let n_samples = if position.is_null() {
            return;
        } else {
            (*position).clock.duration as u32
        };

        if n_samples == 0 {
            return;
        }

        let n_inputs = ctx.input_ports.len().min(MAX_SLOTS);
        let n_outputs = ctx.output_ports.len().min(MAX_SLOTS);

        // Phase 1: Dequeue all output buffers ONCE, zero them, cache pointers.
        let mut out_ptrs = [[std::ptr::null_mut::<f32>(); NUM_CHANNELS]; MAX_SLOTS];
        for (oidx, output_ports) in ctx.output_ports.iter().enumerate().take(MAX_SLOTS) {
            for (ch, &port_data) in output_ports.iter().enumerate() {
                if port_data.is_null() {
                    continue;
                }
                let buf = dequeue_raw_buffer(port_data, n_samples, true);
                if !buf.is_null() {
                    std::ptr::write_bytes(buf, 0, n_samples as usize);
                }
                out_ptrs[oidx][ch] = buf;
            }
        }

        // Phase 2: Dequeue all input buffers ONCE, cache pointers.
        let mut in_ptrs = [[std::ptr::null::<f32>(); NUM_CHANNELS]; MAX_SLOTS];
        for (iidx, input_ports) in ctx.input_ports.iter().enumerate().take(MAX_SLOTS) {
            for (ch, &port_data) in input_ports.iter().enumerate() {
                if port_data.is_null() {
                    continue;
                }
                in_ptrs[iidx][ch] =
                    dequeue_raw_buffer(port_data, n_samples, false) as *const f32;
            }
        }

        // Phase 3: Mix using cached pointers — no dequeue calls.
        for iidx in 0..n_inputs {
            for oidx in 0..n_outputs {
                let vol = ctx.volume_matrix.get(iidx, oidx);
                if vol == 0.0 {
                    continue;
                }
                for ch in 0..NUM_CHANNELS {
                    let in_buf = in_ptrs[iidx][ch];
                    let out_buf = out_ptrs[oidx][ch];
                    if in_buf.is_null() || out_buf.is_null() {
                        continue;
                    }
                    for s in 0..n_samples as usize {
                        *out_buf.add(s) += *in_buf.add(s) * vol;
                    }
                }
            }
        }
    }
}

/// State-changed callback for debug logging.
///
/// # Safety
/// Called from PW thread.
unsafe extern "C" fn mixer_state_changed(
    _data: *mut c_void,
    _old: pw::sys::pw_filter_state,
    state: pw::sys::pw_filter_state,
    error: *const std::os::raw::c_char,
) {
    if state == pw::sys::pw_filter_state_PW_FILTER_STATE_ERROR {
        let err = if error.is_null() {
            "unknown"
        } else {
            unsafe { std::ffi::CStr::from_ptr(error).to_str().unwrap_or("unknown") }
        };
        error!("mixer filter error: {err}");
    } else {
        debug!("mixer filter state: {state}");
    }
}
