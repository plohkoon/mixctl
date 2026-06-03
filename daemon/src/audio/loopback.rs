//! `module-loopback` wrapper used to bridge mixctl output sinks to hardware
//! target devices.
//!
//! Raw port-to-port links between a mixctl output sink's monitor ports and a
//! hardware sink's playback ports work only when both sides run at the same
//! sample rate and format. Devices like the HTC Vive's USB DAC are locked to
//! 44.1 kHz S16LE; the mixctl chain runs at 48 kHz F32P. Linking those
//! directly succeeds at the graph level but produces no audible output —
//! PipeWire does not insert a resampler on bare port-to-port links.
//!
//! `libpipewire-module-loopback` is the built-in adapter for exactly this
//! case: a node pair with format negotiation and resampling on both sides.
//! Loading it for each (output, device) binding fixes the rate-mismatch
//! problem and incidentally handles channel-count differences (8ch mixctl
//! chain → 2ch device) via PipeWire's standard downmix.

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
                node.passive     = true
            }}
            playback.props = {{
                node.name        = "{pb_name}"
                node.description = "{desc}"
                media.class      = "Stream/Output/Audio"
                node.target      = "{target_device_name}"
                node.passive     = true
                resample.disable = false
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
