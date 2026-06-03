//! `module-loopback` wrappers used to bridge the mixctl mixer graph to hardware
//! devices in both directions: outputs ([`build_args`]) and capture inputs
//! ([`build_capture_args`]).
//!
//! Raw port-to-port links between mixctl's null-sinks and hardware ports work
//! only when both sides run at the same sample rate and format. Devices like
//! the HTC Vive's USB DAC are locked to 44.1 kHz S16LE; the mixctl chain runs
//! at 48 kHz F32P. Linking those directly succeeds at the graph level but
//! produces no audible output — PipeWire does not insert a resampler on bare
//! port-to-port links.
//!
//! `libpipewire-module-loopback` is the built-in adapter for exactly this case:
//! a node pair with format negotiation and resampling on both sides. Loading it
//! for each binding fixes the rate-mismatch problem and incidentally handles
//! channel-count differences (8ch mixctl chain ↔ 2ch device) via PipeWire's
//! standard remix.
//!
//! Just as importantly, the loopback's internal resampler is a **clock-domain
//! boundary**. Bridging *every* hardware device through a loopback — capture as
//! well as playback — keeps each device on its own clock and keeps the mixer
//! graph on a single internal clock. Without this, raw-linking a duplex USB
//! device (mic + speaker sharing one clock, e.g. the Blue Yeti) into the mixer
//! lets PipeWire collapse the whole graph onto that device's capture clock and
//! stall all playback.

use std::ffi::CString;
use std::ptr;

use pipewire as pw;
use tracing::error;

/// RAII handle for a loaded `module-loopback` instance. Dropping it calls
/// `pw_impl_module_destroy`, which tears down the loopback's capture/playback
/// node pair.
pub struct LoopbackModule {
    module: *mut pw::sys::pw_impl_module,
}

// The module pointer is only ever touched from the PipeWire thread; this just
// satisfies the borrow checker for owning the handle in `PwState`, which lives
// on that thread.
unsafe impl Send for LoopbackModule {}

impl LoopbackModule {
    /// Load `libpipewire-module-loopback` with the given SPA-JSON `args`.
    /// Returns `None` if the module fails to load (most commonly because the
    /// shared library is missing from the PipeWire install).
    pub fn load(context: &pw::context::ContextRc, args: &str) -> Option<Self> {
        let name = CString::new("libpipewire-module-loopback").ok()?;
        let args_c = CString::new(args).ok()?;
        let module = unsafe {
            pw::sys::pw_context_load_module(
                context.as_raw_ptr(),
                name.as_ptr(),
                args_c.as_ptr(),
                ptr::null_mut(),
            )
        };
        if module.is_null() {
            error!("pw_context_load_module(libpipewire-module-loopback) returned null");
            None
        } else {
            Some(LoopbackModule { module })
        }
    }
}

impl Drop for LoopbackModule {
    fn drop(&mut self) {
        if !self.module.is_null() {
            unsafe { pw::sys::pw_impl_module_destroy(self.module) };
            self.module = ptr::null_mut();
        }
    }
}

/// Build the SPA-JSON args for a loopback that captures from `mixctl.output.{output_id}`
/// and plays back to `target_device_name`. The internal channel layout matches
/// the mixctl chain (8-channel surround); PipeWire downmixes on the playback
/// side if the target has fewer channels.
///
/// Reserved for **rate-mismatched** output devices (e.g. the HTC Vive @ 44.1 kHz).
/// Same-rate devices are bridged with direct `monitor → playback` links (see
/// `queue_output_target_links` in `engine.rs`) so the hardware sink stays inside
/// the mixer's graph component and anchors the clock; a loopback would push it
/// into its own clock domain. Re-enable per-device once we detect device rate
/// and choose direct-vs-loopback accordingly.
#[allow(dead_code)]
pub fn build_args(output_id: u32, target_device_name: &str, target_description: &str) -> String {
    let slug = sanitize_node_name_fragment(target_device_name);
    let cap_name = format!("mixctl.loopback.{output_id}.{slug}.capture");
    let pb_name = format!("mixctl.loopback.{output_id}.{slug}.playback");
    let desc = format!("MixCtl {output_id} → {target_description}");

    // SPA-JSON: double-quote every string so device names with `.` and `-`
    // are unambiguous. Keep the structure flat to make troubleshooting in
    // `pw-dump` output straightforward.
    //
    // `resample.disable = false` on the playback side is the load-bearing bit
    // for fixed-rate sinks (Vive USB DAC @ 44.1 kHz). module-loopback defaults
    // it to true, which works fine when the target is 48 kHz like the rest of
    // the chain but produces silence on rate-mismatched targets — there's no
    // resampler inserted in the adapter to bridge 48 k → 44.1 k.
    //
    // Neither side is `node.passive`. Passive loopback halves were observed in
    // `pw-top` stuck unnegotiated (format rate 0) — they never start, so no
    // audio ever crosses the bridge. Both halves run actively: the playback
    // side keeps the hardware device pulling, the capture side keeps reading our
    // output null-sink's monitor.
    format!(
        r#"{{
            audio.position = [ FL FR FC LFE RL RR SL SR ]
            capture.props = {{
                node.name        = "{cap_name}"
                node.description = "{desc}"
                media.class      = "Stream/Input/Audio"
                node.target      = "mixctl.output.{output_id}"
                stream.capture.sink = true
                stream.dont-remix = true
            }}
            playback.props = {{
                node.name        = "{pb_name}"
                node.description = "{desc}"
                media.class      = "Stream/Output/Audio"
                node.target      = "{target_device_name}"
                resample.disable = false
            }}
        }}"#,
    )
}

/// Build the SPA-JSON args for a loopback that captures from a hardware source
/// `source_device_name` (a microphone / line-in) and plays into
/// `mixctl.input.{input_id}`. This is the capture-direction mirror of
/// [`build_args`].
///
/// Bridging the mic through a loopback — rather than raw port links straight
/// into the input sink — is what keeps the hardware capture device in its own
/// clock domain. A duplex USB device like the Blue Yeti exposes a capture and a
/// playback node that share one clock; when the mic is raw-linked into the
/// mixer graph, PipeWire merges everything (mixer, null-sinks, every output
/// loopback, the playback hardware) into one driver group and elects the Yeti's
/// *capture* side as the driver for the entire graph. A glitchy USB capture
/// clock then stalls all playback. The loopback's internal resampler severs
/// that coupling: the mic side follows the mic's clock, the sink side follows
/// the internal chain, and neither can drive the other.
///
/// The internal layout is stereo (FL/FR) to match a typical mic and the prior
/// raw-link behaviour, which only wired `capture_FL`/`capture_FR` into the input
/// sink's `playback_FL`/`playback_FR`. `stream.dont-remix` keeps that positional
/// mapping when feeding the 8-channel input sink.
pub fn build_capture_args(input_id: u32, source_device_name: &str, source_description: &str) -> String {
    let slug = sanitize_node_name_fragment(source_device_name);
    let cap_name = format!("mixctl.capture.{input_id}.{slug}.capture");
    let pb_name = format!("mixctl.capture.{input_id}.{slug}.playback");
    let desc = format!("MixCtl {source_description} → input {input_id}");

    // `resample.disable = false` lives on the capture side here (the
    // hardware-facing half) so mics locked to a non-48 kHz rate are bridged to
    // the 48 kHz chain, mirroring how `build_args` puts it on its hardware-
    // facing playback side.
    format!(
        r#"{{
            audio.position = [ FL FR ]
            capture.props = {{
                node.name        = "{cap_name}"
                node.description = "{desc}"
                media.class      = "Stream/Input/Audio"
                node.target      = "{source_device_name}"
                resample.disable = false
            }}
            playback.props = {{
                node.name        = "{pb_name}"
                node.description = "{desc}"
                media.class      = "Stream/Output/Audio"
                node.target      = "mixctl.input.{input_id}"
                stream.dont-remix = true
            }}
        }}"#,
    )
}

/// Replace characters that are problematic in PipeWire node names. Node names
/// allow alphanumerics plus `.` `-` `_`; anything else gets replaced with `_`.
fn sanitize_node_name_fragment(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
