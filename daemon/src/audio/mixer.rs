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
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use pipewire as pw;
use tracing::{debug, error, warn};

/// 8 channels: FL, FR, FC, LFE, RL, RR, SL, SR
pub const NUM_CHANNELS: usize = 8;
pub const CHANNELS: [&str; NUM_CHANNELS] = ["FL", "FR", "FC", "LFE", "RL", "RR", "SL", "SR"];

/// Maximum number of inputs and outputs the volume matrix supports.
pub const MAX_SLOTS: usize = 16;

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
///
/// Port pointers are stored in fixed-size arrays with atomic counts to avoid
/// Vec reallocation hazards. The PW main thread writes port pointers and updates
/// counts with Release ordering; the RT process callback reads counts with
/// Acquire ordering and accesses only indices below the count.
struct MixerCallbackData {
    volume_matrix: Arc<VolumeMatrix>,
    /// input_ports[input_idx][channel] = port_data pointer (AtomicPtr for safe cross-thread access)
    input_ports: [[AtomicPtr<c_void>; NUM_CHANNELS]; MAX_SLOTS],
    /// Number of active input slots
    input_count: AtomicUsize,
    /// output_ports[output_idx][channel] = port_data pointer
    output_ports: [[AtomicPtr<c_void>; NUM_CHANNELS]; MAX_SLOTS],
    /// Number of active output slots
    output_count: AtomicUsize,
    /// Per-channel DSP processors (EQ, gate, de-esser, compressor, limiter)
    dsp: super::dsp::DspState,
}

// Safe: AtomicPtr and AtomicUsize are Send+Sync, Arc<VolumeMatrix> is Send+Sync.
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
            input_ports: std::array::from_fn(|_| {
                std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut()))
            }),
            input_count: AtomicUsize::new(0),
            output_ports: std::array::from_fn(|_| {
                std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut()))
            }),
            output_count: AtomicUsize::new(0),
            dsp: super::dsp::DspState::default(),
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
            cb.input_ports[idx][ch_idx].store(port_data, Ordering::Release);
        }

        cb.input_count.store(idx + 1, Ordering::Release);
        self.num_inputs += 1;
        debug!("mixer: added input ports for input {input_id} (idx={idx})");
        idx
    }

    /// Add 8 output ports for a logical output (one per channel).
    /// Returns the output index (0-based) used in the volume matrix.
    pub fn add_output_ports(&mut self, output_id: u32) -> usize {
        let idx = self.num_outputs;
        let cb = unsafe { &mut *self.callback_data };

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
            cb.output_ports[idx][ch_idx].store(port_data, Ordering::Release);
        }

        cb.output_count.store(idx + 1, Ordering::Release);
        self.num_outputs += 1;
        debug!("mixer: added output ports for output {output_id} (idx={idx})");
        idx
    }

    /// Remove 8 input ports for a logical input at the given index.
    pub fn remove_input_ports(&mut self, idx: usize) {
        let cb = unsafe { &mut *self.callback_data };
        let count = cb.input_count.load(Ordering::Acquire);
        if idx >= count {
            return;
        }

        // Collect the port pointers at `idx` for removal, then null them out.
        let mut removed = [std::ptr::null_mut(); NUM_CHANNELS];
        for ch in 0..NUM_CHANNELS {
            removed[ch] = cb.input_ports[idx][ch].swap(std::ptr::null_mut(), Ordering::Release);
        }

        // Shift remaining slots down: idx+1..count -> idx..count-1
        for slot in idx..count - 1 {
            for ch in 0..NUM_CHANNELS {
                let ptr = cb.input_ports[slot + 1][ch].swap(std::ptr::null_mut(), Ordering::Release);
                cb.input_ports[slot][ch].store(ptr, Ordering::Release);
            }
        }

        // Decrement count (RT thread will see fewer slots)
        cb.input_count.store(count - 1, Ordering::Release);
        self.num_inputs = self.num_inputs.saturating_sub(1);

        // Now actually remove the PipeWire ports
        for port_data in removed {
            if !port_data.is_null() {
                unsafe { pw::sys::pw_filter_remove_port(port_data); }
            }
        }
    }

    /// Remove 8 output ports for a logical output at the given index.
    pub fn remove_output_ports(&mut self, idx: usize) {
        let cb = unsafe { &mut *self.callback_data };
        let count = cb.output_count.load(Ordering::Acquire);
        if idx >= count {
            return;
        }

        // Collect the port pointers at `idx` for removal, then null them out.
        let mut removed = [std::ptr::null_mut(); NUM_CHANNELS];
        for ch in 0..NUM_CHANNELS {
            removed[ch] = cb.output_ports[idx][ch].swap(std::ptr::null_mut(), Ordering::Release);
        }

        // Shift remaining slots down: idx+1..count -> idx..count-1
        for slot in idx..count - 1 {
            for ch in 0..NUM_CHANNELS {
                let ptr = cb.output_ports[slot + 1][ch].swap(std::ptr::null_mut(), Ordering::Release);
                cb.output_ports[slot][ch].store(ptr, Ordering::Release);
            }
        }

        // Decrement count (RT thread will see fewer slots)
        cb.output_count.store(count - 1, Ordering::Release);
        self.num_outputs = self.num_outputs.saturating_sub(1);

        // Now actually remove the PipeWire ports
        for port_data in removed {
            if !port_data.is_null() {
                unsafe { pw::sys::pw_filter_remove_port(port_data); }
            }
        }
    }

    // -- DSP accessor methods --

    pub fn set_input_eq_enabled(&self, idx: usize, enabled: bool) {
        let cb = unsafe { &mut *self.callback_data };
        if idx < MAX_SLOTS {
            cb.dsp.input_eq[idx].enabled.store(enabled, Ordering::Release);
        }
    }

    pub fn set_input_eq_band(
        &self,
        idx: usize,
        band: usize,
        band_type: super::dsp::EqBandType,
        freq: f32,
        gain_db: f32,
        q: f32,
        sample_rate: f32,
    ) {
        let cb = unsafe { &mut *self.callback_data };
        if idx >= MAX_SLOTS || band >= super::dsp::NUM_EQ_BANDS {
            return;
        }
        let coeffs = match band_type {
            super::dsp::EqBandType::LowShelf => {
                super::dsp::BiquadCoeffs::low_shelf(freq, gain_db, q, sample_rate)
            }
            super::dsp::EqBandType::Peaking => {
                super::dsp::BiquadCoeffs::peaking(freq, gain_db, q, sample_rate)
            }
            super::dsp::EqBandType::HighShelf => {
                super::dsp::BiquadCoeffs::high_shelf(freq, gain_db, q, sample_rate)
            }
            super::dsp::EqBandType::Bypass => super::dsp::BiquadCoeffs::default(),
        };
        for ch in 0..NUM_CHANNELS {
            cb.dsp.input_eq[idx].coeffs[ch][band] = coeffs;
            cb.dsp.input_eq[idx].state[ch][band] = super::dsp::BiquadState::default();
        }
    }

    pub fn get_input_eq(
        &self,
        idx: usize,
    ) -> Option<bool> {
        let cb = unsafe { &*self.callback_data };
        if idx >= MAX_SLOTS {
            return None;
        }
        Some(cb.dsp.input_eq[idx].enabled.load(Ordering::Acquire))
    }

    pub fn reset_input_eq(&self, idx: usize) {
        let cb = unsafe { &mut *self.callback_data };
        if idx >= MAX_SLOTS {
            return;
        }
        for ch in 0..NUM_CHANNELS {
            for band in 0..super::dsp::NUM_EQ_BANDS {
                cb.dsp.input_eq[idx].coeffs[ch][band] = super::dsp::BiquadCoeffs::default();
                cb.dsp.input_eq[idx].state[ch][band] = super::dsp::BiquadState::default();
            }
        }
    }

    pub fn set_input_gate_enabled(&self, idx: usize, enabled: bool) {
        let cb = unsafe { &mut *self.callback_data };
        if idx < MAX_SLOTS {
            cb.dsp.input_gate[idx].enabled.store(enabled, Ordering::Release);
        }
    }

    pub fn set_input_gate(
        &self,
        idx: usize,
        threshold_db: f64,
        attack_ms: f64,
        release_ms: f64,
        hold_ms: f64,
        sample_rate: f32,
    ) {
        let cb = unsafe { &mut *self.callback_data };
        if idx >= MAX_SLOTS {
            return;
        }
        cb.dsp.input_gate[idx].config.threshold_linear =
            super::dsp::db_to_linear(threshold_db);
        cb.dsp.input_gate[idx].config.attack_coeff =
            super::dsp::time_constant(attack_ms, sample_rate);
        cb.dsp.input_gate[idx].config.release_coeff =
            super::dsp::time_constant(release_ms, sample_rate);
        cb.dsp.input_gate[idx].config.hold_samples =
            (hold_ms as f32 * 0.001 * sample_rate) as u32;
    }

    pub fn set_input_deesser_enabled(&self, idx: usize, enabled: bool) {
        let cb = unsafe { &mut *self.callback_data };
        if idx < MAX_SLOTS {
            cb.dsp.input_deesser[idx].enabled.store(enabled, Ordering::Release);
        }
    }

    pub fn set_input_deesser(
        &self,
        idx: usize,
        freq: f32,
        threshold_db: f64,
        ratio: f64,
        sample_rate: f32,
    ) {
        let cb = unsafe { &mut *self.callback_data };
        if idx >= MAX_SLOTS {
            return;
        }
        cb.dsp.input_deesser[idx].threshold_linear =
            super::dsp::db_to_linear(threshold_db);
        cb.dsp.input_deesser[idx].ratio = ratio as f32;
        cb.dsp.input_deesser[idx].hpf_coeffs =
            super::dsp::BiquadCoeffs::high_pass(freq, 0.7, sample_rate);
        // Reset filter state
        cb.dsp.input_deesser[idx].hpf_state =
            [super::dsp::BiquadState::default(); NUM_CHANNELS];
    }

    pub fn set_output_compressor_enabled(&self, idx: usize, enabled: bool) {
        let cb = unsafe { &mut *self.callback_data };
        if idx < MAX_SLOTS {
            cb.dsp.output_compressor[idx]
                .enabled
                .store(enabled, Ordering::Release);
        }
    }

    pub fn set_output_compressor(
        &self,
        idx: usize,
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
        makeup_gain_db: f64,
        knee_db: f64,
        sample_rate: f32,
    ) {
        let cb = unsafe { &mut *self.callback_data };
        if idx >= MAX_SLOTS {
            return;
        }
        cb.dsp.output_compressor[idx].threshold_linear =
            super::dsp::db_to_linear(threshold_db);
        cb.dsp.output_compressor[idx].ratio = ratio as f32;
        cb.dsp.output_compressor[idx].attack_coeff =
            super::dsp::time_constant(attack_ms, sample_rate);
        cb.dsp.output_compressor[idx].release_coeff =
            super::dsp::time_constant(release_ms, sample_rate);
        cb.dsp.output_compressor[idx].makeup_gain =
            super::dsp::db_to_linear(makeup_gain_db);
        cb.dsp.output_compressor[idx].knee_width =
            super::dsp::db_to_linear(knee_db);
    }

    pub fn set_output_limiter_enabled(&self, idx: usize, enabled: bool) {
        let cb = unsafe { &mut *self.callback_data };
        if idx < MAX_SLOTS {
            cb.dsp.output_limiter[idx]
                .enabled
                .store(enabled, Ordering::Release);
        }
    }

    pub fn set_output_limiter(
        &self,
        idx: usize,
        ceiling_db: f64,
        release_ms: f64,
        sample_rate: f32,
    ) {
        let cb = unsafe { &mut *self.callback_data };
        if idx >= MAX_SLOTS {
            return;
        }
        cb.dsp.output_limiter[idx].ceiling_linear =
            super::dsp::db_to_linear(ceiling_db);
        cb.dsp.output_limiter[idx].release_coeff =
            super::dsp::time_constant(release_ms, sample_rate);
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
        // Mutable reference: DSP state needs mutation in the RT callback.
        // Safe because the RT callback is single-threaded.
        let ctx = &mut *(data as *mut MixerCallbackData);
        let n_samples = if position.is_null() {
            return;
        } else {
            (*position).clock.duration as u32
        };

        if n_samples == 0 {
            return;
        }

        let n_inputs = ctx.input_count.load(Ordering::Acquire).min(MAX_SLOTS);
        let n_outputs = ctx.output_count.load(Ordering::Acquire).min(MAX_SLOTS);

        // Phase 1: Dequeue all output buffers ONCE, zero them, cache pointers.
        let mut out_ptrs = [[std::ptr::null_mut::<f32>(); NUM_CHANNELS]; MAX_SLOTS];
        for oidx in 0..n_outputs {
            for ch in 0..NUM_CHANNELS {
                let port_data = ctx.output_ports[oidx][ch].load(Ordering::Acquire);
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
        let mut in_ptrs = [[std::ptr::null_mut::<f32>(); NUM_CHANNELS]; MAX_SLOTS];
        for iidx in 0..n_inputs {
            for ch in 0..NUM_CHANNELS {
                let port_data = ctx.input_ports[iidx][ch].load(Ordering::Acquire);
                if port_data.is_null() {
                    continue;
                }
                in_ptrs[iidx][ch] =
                    dequeue_raw_buffer(port_data, n_samples, false);
            }
        }

        // Phase 2.5: Per-input DSP (EQ → Gate → De-esser) — all toggleable
        for iidx in 0..n_inputs {
            for ch in 0..NUM_CHANNELS {
                let buf = in_ptrs[iidx][ch];
                if buf.is_null() {
                    continue;
                }
                super::dsp::apply_eq(&mut ctx.dsp.input_eq[iidx], ch, buf, n_samples);
                super::dsp::apply_gate(&mut ctx.dsp.input_gate[iidx], ch, buf, n_samples);
                super::dsp::apply_deesser(&mut ctx.dsp.input_deesser[iidx], ch, buf, n_samples);
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
                    let in_buf = in_ptrs[iidx][ch] as *const f32;
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

        // Phase 4: Per-output DSP (Compressor → Limiter) — all toggleable
        for oidx in 0..n_outputs {
            for ch in 0..NUM_CHANNELS {
                let buf = out_ptrs[oidx][ch];
                if buf.is_null() {
                    continue;
                }
                super::dsp::apply_compressor(&mut ctx.dsp.output_compressor[oidx], ch, buf, n_samples);
                super::dsp::apply_limiter(&mut ctx.dsp.output_limiter[oidx], ch, buf, n_samples);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_matrix_is_all_zeros() {
        let m = VolumeMatrix::new();
        for i in 0..MAX_SLOTS {
            for o in 0..MAX_SLOTS {
                assert_eq!(m.get(i, o), 0.0, "expected 0.0 at ({i}, {o})");
            }
        }
    }

    #[test]
    fn set_get_origin() {
        let m = VolumeMatrix::new();
        m.set(0, 0, 0.75);
        assert_eq!(m.get(0, 0), 0.75);
    }

    #[test]
    fn set_get_max_corner() {
        let m = VolumeMatrix::new();
        m.set(MAX_SLOTS - 1, MAX_SLOTS - 1, 0.42);
        assert_eq!(m.get(MAX_SLOTS - 1, MAX_SLOTS - 1), 0.42);
    }

    #[test]
    fn out_of_bounds_get_returns_zero() {
        let m = VolumeMatrix::new();
        m.set(0, 0, 1.0);
        assert_eq!(m.get(MAX_SLOTS, 0), 0.0);
        assert_eq!(m.get(0, MAX_SLOTS), 0.0);
        assert_eq!(m.get(MAX_SLOTS, MAX_SLOTS), 0.0);
        assert_eq!(m.get(usize::MAX, usize::MAX), 0.0);
    }

    #[test]
    fn out_of_bounds_set_is_noop() {
        let m = VolumeMatrix::new();
        // These should not panic
        m.set(MAX_SLOTS, 0, 1.0);
        m.set(0, MAX_SLOTS, 1.0);
        m.set(MAX_SLOTS, MAX_SLOTS, 1.0);
        m.set(usize::MAX, usize::MAX, 1.0);
        // And nothing was written to valid cells
        assert_eq!(m.get(0, 0), 0.0);
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
