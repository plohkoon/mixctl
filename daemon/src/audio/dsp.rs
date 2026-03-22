//! Per-channel DSP processors: parametric EQ, noise gate, de-esser, compressor, limiter.
//!
//! All processors are designed for real-time use in the PW process callback:
//! - No allocations
//! - No blocking
//! - Fixed-size state arrays
//! - Each processor has an `enabled` flag; when false, processing is a no-op (zero CPU)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::mixer::{MAX_SLOTS, NUM_CHANNELS};

// ---------------------------------------------------------------------------
// Biquad filter (building block for EQ and de-esser)
// ---------------------------------------------------------------------------

/// Second-order IIR (biquad) filter coefficients.
#[derive(Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        // Unity pass-through (no filtering)
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
    }
}

impl BiquadCoeffs {
    /// Peaking EQ filter.
    pub fn peaking(freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos_w0) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    /// Low shelf filter.
    pub fn low_shelf(freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        Self {
            b0: (a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) / a0,
            b1: (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0,
            b2: (a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) / a0,
            a1: (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0,
            a2: ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0,
        }
    }

    /// High shelf filter.
    pub fn high_shelf(freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        Self {
            b0: (a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) / a0,
            b1: (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0,
            b2: (a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) / a0,
            a1: (2.0 * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0,
            a2: ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0,
        }
    }

    /// High-pass filter (used by de-esser sidechain).
    pub fn high_pass(freq: f32, q: f32, sample_rate: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cos_w0) / 2.0) / a0,
            b1: (-(1.0 + cos_w0)) / a0,
            b2: ((1.0 + cos_w0) / 2.0) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

/// Biquad filter state (2 samples of history).
#[derive(Clone, Copy, Default)]
pub struct BiquadState {
    pub x1: f32,
    pub x2: f32,
    pub y1: f32,
    pub y2: f32,
}

impl BiquadState {
    /// Process a single sample through the biquad filter.
    #[inline]
    pub fn process(&mut self, coeffs: &BiquadCoeffs, x: f32) -> f32 {
        let y = coeffs.b0 * x + coeffs.b1 * self.x1 + coeffs.b2 * self.x2
            - coeffs.a1 * self.y1 - coeffs.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

// ---------------------------------------------------------------------------
// EQ: 8-band parametric equalizer per input
// ---------------------------------------------------------------------------

pub const NUM_EQ_BANDS: usize = 8;

/// Default EQ band frequencies.
pub const DEFAULT_EQ_FREQS: [f32; NUM_EQ_BANDS] = [
    80.0, 250.0, 800.0, 2500.0, 5000.0, 8000.0, 12000.0, 16000.0,
];

/// EQ band type.
#[derive(Clone, Copy, PartialEq)]
pub enum EqBandType {
    LowShelf,
    Peaking,
    HighShelf,
    Bypass,
}

/// Configuration for one EQ band.
#[derive(Clone, Copy)]
pub struct EqBandConfig {
    pub band_type: EqBandType,
    pub frequency: f32,
    pub gain_db: f32,
    pub q: f32,
}

impl Default for EqBandConfig {
    fn default() -> Self {
        Self {
            band_type: EqBandType::Peaking,
            frequency: 1000.0,
            gain_db: 0.0,
            q: 1.4,
        }
    }
}

/// Per-input EQ state: 8 biquad filters × NUM_CHANNELS.
pub struct InputEqState {
    pub enabled: AtomicBool,
    pub coeffs: [[BiquadCoeffs; NUM_EQ_BANDS]; NUM_CHANNELS],
    pub state: [[BiquadState; NUM_EQ_BANDS]; NUM_CHANNELS],
}

impl Default for InputEqState {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            coeffs: [[BiquadCoeffs::default(); NUM_EQ_BANDS]; NUM_CHANNELS],
            state: [[BiquadState::default(); NUM_EQ_BANDS]; NUM_CHANNELS],
        }
    }
}

/// Apply 8-band EQ to a buffer in-place.
#[inline]
pub fn apply_eq(eq: &mut InputEqState, ch: usize, buf: *mut f32, n_samples: u32) {
    if !eq.enabled.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        for s in 0..n_samples as usize {
            let mut sample = *buf.add(s);
            for band in 0..NUM_EQ_BANDS {
                if eq.coeffs[ch][band].b0 == 1.0
                    && eq.coeffs[ch][band].b1 == 0.0
                    && eq.coeffs[ch][band].a1 == 0.0
                {
                    continue; // bypass band (unity coefficients)
                }
                sample = eq.state[ch][band].process(&eq.coeffs[ch][band], sample);
            }
            *buf.add(s) = sample;
        }
    }
}

// ---------------------------------------------------------------------------
// Noise Gate per input
// ---------------------------------------------------------------------------

/// Noise gate configuration.
#[derive(Clone, Copy)]
pub struct GateConfig {
    pub threshold_linear: f32, // pre-computed from threshold_db
    pub attack_coeff: f32,     // pre-computed from attack_ms + sample_rate
    pub release_coeff: f32,    // pre-computed from release_ms + sample_rate
    pub hold_samples: u32,     // pre-computed from hold_ms + sample_rate
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            threshold_linear: 0.01, // ~-40 dB
            attack_coeff: 0.99,
            release_coeff: 0.9999,
            hold_samples: 2400, // 50ms at 48kHz
        }
    }
}

/// Per-input gate state.
pub struct InputGateState {
    pub enabled: AtomicBool,
    pub config: GateConfig,
    pub envelope: [f32; NUM_CHANNELS],
    pub hold_counter: [u32; NUM_CHANNELS],
    pub gain: [f32; NUM_CHANNELS],
}

impl Default for InputGateState {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            config: GateConfig::default(),
            envelope: [0.0; NUM_CHANNELS],
            hold_counter: [0; NUM_CHANNELS],
            gain: [1.0; NUM_CHANNELS],
        }
    }
}

/// Apply noise gate to a buffer in-place.
#[inline]
pub fn apply_gate(gate: &mut InputGateState, ch: usize, buf: *mut f32, n_samples: u32) {
    if !gate.enabled.load(Ordering::Relaxed) {
        return;
    }
    let cfg = &gate.config;
    unsafe {
        for s in 0..n_samples as usize {
            let sample = *buf.add(s);
            let abs_sample = sample.abs();

            // Envelope follower
            if abs_sample > gate.envelope[ch] {
                gate.envelope[ch] = cfg.attack_coeff * gate.envelope[ch]
                    + (1.0 - cfg.attack_coeff) * abs_sample;
            } else {
                gate.envelope[ch] = cfg.release_coeff * gate.envelope[ch]
                    + (1.0 - cfg.release_coeff) * abs_sample;
            }

            // Gate logic with hold
            if gate.envelope[ch] > cfg.threshold_linear {
                gate.hold_counter[ch] = cfg.hold_samples;
                gate.gain[ch] = 1.0;
            } else if gate.hold_counter[ch] > 0 {
                gate.hold_counter[ch] -= 1;
            } else {
                // Smooth close
                gate.gain[ch] *= 0.999;
            }

            *buf.add(s) = sample * gate.gain[ch];
        }
    }
}

// ---------------------------------------------------------------------------
// De-esser per input
// ---------------------------------------------------------------------------

/// Per-input de-esser state.
pub struct InputDeesserState {
    pub enabled: AtomicBool,
    pub threshold_linear: f32,
    pub ratio: f32,
    pub hpf_coeffs: BiquadCoeffs,
    pub hpf_state: [BiquadState; NUM_CHANNELS],
    pub envelope: [f32; NUM_CHANNELS],
}

impl Default for InputDeesserState {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            threshold_linear: 0.1, // ~-20 dB
            ratio: 4.0,
            hpf_coeffs: BiquadCoeffs::high_pass(6000.0, 0.7, 48000.0),
            hpf_state: [BiquadState::default(); NUM_CHANNELS],
            envelope: [0.0; NUM_CHANNELS],
        }
    }
}

/// Apply de-esser to a buffer in-place.
#[inline]
pub fn apply_deesser(ds: &mut InputDeesserState, ch: usize, buf: *mut f32, n_samples: u32) {
    if !ds.enabled.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        for s in 0..n_samples as usize {
            let sample = *buf.add(s);

            // Sidechain: high-pass filter to isolate sibilance
            let sidechain = ds.hpf_state[ch].process(&ds.hpf_coeffs, sample);
            let sc_abs = sidechain.abs();

            // Envelope on sidechain
            let attack = 0.995_f32;
            let release = 0.9999_f32;
            if sc_abs > ds.envelope[ch] {
                ds.envelope[ch] = attack * ds.envelope[ch] + (1.0 - attack) * sc_abs;
            } else {
                ds.envelope[ch] = release * ds.envelope[ch] + (1.0 - release) * sc_abs;
            }

            // Gain reduction when sibilance exceeds threshold
            if ds.envelope[ch] > ds.threshold_linear {
                let over = ds.envelope[ch] / ds.threshold_linear;
                let gain_reduction = over.powf(1.0 / ds.ratio - 1.0);
                *buf.add(s) = sample * gain_reduction;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Compressor per output
// ---------------------------------------------------------------------------

/// Per-output compressor state.
pub struct OutputCompressorState {
    pub enabled: AtomicBool,
    pub threshold_linear: f32,
    pub ratio: f32,
    pub attack_coeff: f32,
    pub release_coeff: f32,
    pub makeup_gain: f32,    // linear
    pub knee_width: f32,     // in linear amplitude
    pub envelope: [f32; NUM_CHANNELS],
}

impl Default for OutputCompressorState {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            threshold_linear: 0.125, // ~-18 dB
            ratio: 4.0,
            attack_coeff: 0.9995,
            release_coeff: 0.99995,
            makeup_gain: 1.0,
            knee_width: 0.05,
            envelope: [0.0; NUM_CHANNELS],
        }
    }
}

/// Apply compressor to a buffer in-place.
#[inline]
pub fn apply_compressor(comp: &mut OutputCompressorState, ch: usize, buf: *mut f32, n_samples: u32) {
    if !comp.enabled.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        for s in 0..n_samples as usize {
            let sample = *buf.add(s);
            let abs_sample = sample.abs().max(1e-10);

            // Envelope follower
            if abs_sample > comp.envelope[ch] {
                comp.envelope[ch] = comp.attack_coeff * comp.envelope[ch]
                    + (1.0 - comp.attack_coeff) * abs_sample;
            } else {
                comp.envelope[ch] = comp.release_coeff * comp.envelope[ch]
                    + (1.0 - comp.release_coeff) * abs_sample;
            }

            // Gain computer
            let gain = if comp.envelope[ch] > comp.threshold_linear {
                let over = comp.envelope[ch] / comp.threshold_linear;
                let gain_reduction = over.powf(1.0 / comp.ratio - 1.0);
                gain_reduction * comp.makeup_gain
            } else {
                comp.makeup_gain
            };

            *buf.add(s) = sample * gain;
        }
    }
}

// ---------------------------------------------------------------------------
// Limiter per output
// ---------------------------------------------------------------------------

/// Per-output limiter state.
pub struct OutputLimiterState {
    pub enabled: AtomicBool,
    pub ceiling_linear: f32, // pre-computed from ceiling_db
    pub release_coeff: f32,
    pub gain: [f32; NUM_CHANNELS],
}

impl Default for OutputLimiterState {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            ceiling_linear: 0.944, // ~-0.5 dB
            release_coeff: 0.9999,
            gain: [1.0; NUM_CHANNELS],
        }
    }
}

/// Apply brick-wall limiter to a buffer in-place.
#[inline]
pub fn apply_limiter(lim: &mut OutputLimiterState, ch: usize, buf: *mut f32, n_samples: u32) {
    if !lim.enabled.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        for s in 0..n_samples as usize {
            let sample = *buf.add(s);
            let abs_sample = sample.abs();

            if abs_sample > lim.ceiling_linear {
                let target_gain = lim.ceiling_linear / abs_sample;
                if target_gain < lim.gain[ch] {
                    lim.gain[ch] = target_gain; // instant attack
                }
            } else {
                // Release
                lim.gain[ch] = (lim.gain[ch] + (1.0 - lim.gain[ch]) * (1.0 - lim.release_coeff)).min(1.0);
            }

            *buf.add(s) = sample * lim.gain[ch];
        }
    }
}

// ---------------------------------------------------------------------------
// Aggregate DSP state for the mixer
// ---------------------------------------------------------------------------

/// All DSP state for the mixer, stored in MixerCallbackData.
pub struct DspState {
    pub input_eq: [InputEqState; MAX_SLOTS],
    pub input_gate: [InputGateState; MAX_SLOTS],
    pub input_deesser: [InputDeesserState; MAX_SLOTS],
    pub output_compressor: [OutputCompressorState; MAX_SLOTS],
    pub output_limiter: [OutputLimiterState; MAX_SLOTS],
}

impl Default for DspState {
    fn default() -> Self {
        Self {
            input_eq: std::array::from_fn(|_| InputEqState::default()),
            input_gate: std::array::from_fn(|_| InputGateState::default()),
            input_deesser: std::array::from_fn(|_| InputDeesserState::default()),
            output_compressor: std::array::from_fn(|_| OutputCompressorState::default()),
            output_limiter: std::array::from_fn(|_| OutputLimiterState::default()),
        }
    }
}

// ---------------------------------------------------------------------------
// Utility: dB to linear conversion
// ---------------------------------------------------------------------------

/// Convert dB to linear amplitude.
pub fn db_to_linear(db: f64) -> f32 {
    10.0_f32.powf(db as f32 / 20.0)
}

/// Compute time constant coefficient from milliseconds and sample rate.
pub fn time_constant(ms: f64, sample_rate: f32) -> f32 {
    (-1.0 / (ms as f32 * 0.001 * sample_rate)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biquad_unity_passthrough() {
        let coeffs = BiquadCoeffs::default();
        let mut state = BiquadState::default();
        let input = 0.5_f32;
        let output = state.process(&coeffs, input);
        assert!((output - input).abs() < 1e-6);
    }

    #[test]
    fn peaking_eq_zero_gain_is_unity() {
        let coeffs = BiquadCoeffs::peaking(1000.0, 0.0, 1.4, 48000.0);
        let mut state = BiquadState::default();
        // After settling, output should equal input
        for _ in 0..100 {
            state.process(&coeffs, 0.5);
        }
        let output = state.process(&coeffs, 0.5);
        assert!((output - 0.5).abs() < 0.01, "output={output}");
    }

    #[test]
    fn db_to_linear_values() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_linear(-20.0) - 0.1).abs() < 0.01);
        assert!((db_to_linear(20.0) - 10.0).abs() < 0.1);
    }

    #[test]
    fn gate_bypassed_when_disabled() {
        let mut gate = InputGateState::default();
        gate.enabled.store(false, Ordering::Relaxed);
        let mut buf = [0.5_f32; 256];
        apply_gate(&mut gate, 0, buf.as_mut_ptr(), 256);
        assert!((buf[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn limiter_clamps_output() {
        let mut lim = OutputLimiterState::default();
        lim.enabled.store(true, Ordering::Relaxed);
        lim.ceiling_linear = 0.5;
        let mut buf = [1.0_f32; 256];
        apply_limiter(&mut lim, 0, buf.as_mut_ptr(), 256);
        for &sample in &buf {
            assert!(sample.abs() <= 0.51, "sample={sample} exceeds ceiling");
        }
    }
}
