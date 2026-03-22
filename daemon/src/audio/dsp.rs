//! Per-channel DSP processors: parametric EQ, noise gate, de-esser, compressor, limiter.
//!
//! All processors are designed for real-time use in the PW process callback:
//! - No allocations
//! - No blocking
//! - Fixed-size state arrays
//! - Each processor has an `enabled` flag; when false, processing is a no-op (zero CPU)

use std::sync::atomic::{AtomicBool, Ordering};

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

impl BiquadCoeffs {
    /// Compute magnitude response in dB at a given frequency.
    #[allow(dead_code)]
    pub fn magnitude_db(&self, freq: f32, sample_rate: f32) -> f32 {
        let w = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sin_w, cos_w) = w.sin_cos();
        let (sin_2w, cos_2w) = (2.0 * w).sin_cos();

        // H(z) = (b0 + b1*z^-1 + b2*z^-2) / (1 + a1*z^-1 + a2*z^-2)
        // at z = e^(jw): z^-1 = cos(w) - j*sin(w), z^-2 = cos(2w) - j*sin(2w)
        let num_re = self.b0 + self.b1 * cos_w + self.b2 * cos_2w;
        let num_im = -(self.b1 * sin_w + self.b2 * sin_2w);
        let den_re = 1.0 + self.a1 * cos_w + self.a2 * cos_2w;
        let den_im = -(self.a1 * sin_w + self.a2 * sin_2w);

        let num_mag_sq = num_re * num_re + num_im * num_im;
        let den_mag_sq = den_re * den_re + den_im * den_im;

        if den_mag_sq < 1e-20 {
            return 0.0;
        }
        10.0 * (num_mag_sq / den_mag_sq).log10()
    }
}

/// Number of points for frequency response curve.
#[allow(dead_code)]
pub const FREQ_RESPONSE_POINTS: usize = 150;

/// Compute the combined frequency response of 8 cascaded EQ bands.
/// Returns (frequency_hz, magnitude_db) pairs on a log scale from 20Hz to 20kHz.
#[allow(dead_code)]
pub fn compute_eq_response(
    coeffs: &[BiquadCoeffs; NUM_EQ_BANDS],
    sample_rate: f32,
) -> Vec<(f32, f32)> {
    let mut points = Vec::with_capacity(FREQ_RESPONSE_POINTS);
    let log_min = 20.0_f32.ln();
    let log_max = 20000.0_f32.ln();

    for i in 0..FREQ_RESPONSE_POINTS {
        let t = i as f32 / (FREQ_RESPONSE_POINTS - 1) as f32;
        let freq = (log_min + t * (log_max - log_min)).exp();

        let mut total_db = 0.0_f32;
        for band in 0..NUM_EQ_BANDS {
            total_db += coeffs[band].magnitude_db(freq, sample_rate);
        }
        // Clamp to ±30 dB for display
        total_db = total_db.clamp(-30.0, 30.0);
        points.push((freq, total_db));
    }
    points
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
    /// Pre-computed exponent: `1.0 / ratio - 1.0` (avoids per-sample powf)
    pub ratio_exponent: f32,
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
            ratio_exponent: 1.0 / 4.0 - 1.0,
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
                let gain_reduction = over.powf(ds.ratio_exponent);
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
    /// Pre-computed exponent: `1.0 / ratio - 1.0` (avoids per-sample powf)
    pub ratio_exponent: f32,
    pub attack_coeff: f32,
    pub release_coeff: f32,
    pub makeup_gain: f32,    // linear
    pub envelope: [f32; NUM_CHANNELS],
}

impl Default for OutputCompressorState {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            threshold_linear: 0.125, // ~-18 dB
            ratio: 4.0,
            ratio_exponent: 1.0 / 4.0 - 1.0,
            attack_coeff: 0.9995,
            release_coeff: 0.99995,
            makeup_gain: 1.0,
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
                let gain_reduction = over.powf(comp.ratio_exponent);
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

    fn sine_wave(freq: f32, sample_rate: f32, n_samples: usize) -> Vec<f32> {
        (0..n_samples)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin())
            .collect()
    }

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt()
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().map(|x| x.abs()).fold(0.0_f32, f32::max)
    }

    #[test]
    fn eq_boost_increases_signal() {
        let sample_rate = 48000.0;
        let n = 4096;
        let input = sine_wave(1000.0, sample_rate, n);
        let input_rms = rms(&input);

        let mut eq = InputEqState::default();
        eq.enabled.store(true, Ordering::Relaxed);
        // Set band 0 to peaking at 1kHz with +12dB
        eq.coeffs[0][0] = BiquadCoeffs::peaking(1000.0, 12.0, 1.4, sample_rate);

        let mut output = input.clone();
        apply_eq(&mut eq, 0, output.as_mut_ptr(), n as u32);

        let output_rms = rms(&output);
        assert!(
            output_rms > input_rms,
            "EQ boost should increase signal: output_rms={output_rms} input_rms={input_rms}"
        );
    }

    #[test]
    fn gate_closes_on_silence() {
        let mut gate = InputGateState::default();
        gate.enabled.store(true, Ordering::Relaxed);
        gate.config.hold_samples = 100;
        gate.gain[0] = 1.0;

        // Feed silence (zeros) for enough samples to exhaust hold
        let n = 10_000;
        let mut buf = vec![0.0_f32; n];
        apply_gate(&mut gate, 0, buf.as_mut_ptr(), n as u32);

        assert!(
            gate.gain[0] < 0.01,
            "gate gain should drop near 0 on silence, got {}",
            gate.gain[0]
        );
    }

    #[test]
    fn gate_opens_on_loud_signal() {
        let mut gate = InputGateState::default();
        gate.enabled.store(true, Ordering::Relaxed);
        gate.config.threshold_linear = 0.01;
        // Start with gate closed
        gate.gain[0] = 0.0;

        // Feed a loud signal
        let mut buf = sine_wave(440.0, 48000.0, 4096);
        apply_gate(&mut gate, 0, buf.as_mut_ptr(), buf.len() as u32);

        // The output should have non-zero samples (gate opened)
        let out_peak = peak(&buf);
        assert!(
            out_peak > 0.1,
            "gate should open on loud signal, got peak={}",
            out_peak
        );
    }

    #[test]
    fn deesser_reduces_high_freq() {
        let sample_rate = 48000.0;
        let n = 8192;
        // Signal above the de-esser frequency and above threshold
        let input = sine_wave(8000.0, sample_rate, n);
        let input_peak = peak(&input);

        let mut ds = InputDeesserState::default();
        ds.enabled.store(true, Ordering::Relaxed);
        ds.threshold_linear = 0.05; // low threshold so the signal triggers reduction
        ds.hpf_coeffs = BiquadCoeffs::high_pass(6000.0, 0.7, sample_rate);

        let mut output = input.clone();
        apply_deesser(&mut ds, 0, output.as_mut_ptr(), n as u32);

        let output_peak = peak(&output);
        assert!(
            output_peak < input_peak,
            "de-esser should reduce high-freq peaks: output_peak={output_peak} input_peak={input_peak}"
        );
    }

    #[test]
    fn compressor_reduces_peaks() {
        let sample_rate = 48000.0;
        let n = 48000; // 1 second of audio to let the envelope settle
        // Loud signal above -18dB threshold (default threshold_linear ~ 0.125)
        let input: Vec<f32> = sine_wave(440.0, sample_rate, n)
            .iter()
            .map(|x| x * 0.8) // ~-2dB, well above threshold
            .collect();

        let mut comp = OutputCompressorState::default();
        comp.enabled.store(true, Ordering::Relaxed);
        comp.threshold_linear = 0.125; // ~-18dB
        comp.makeup_gain = 1.0;

        let mut output = input.clone();
        apply_compressor(&mut comp, 0, output.as_mut_ptr(), n as u32);

        // Check the tail portion where the envelope has settled
        let tail_start = n / 2;
        let input_tail_peak = peak(&input[tail_start..]);
        let output_tail_peak = peak(&output[tail_start..]);
        assert!(
            output_tail_peak < input_tail_peak,
            "compressor should reduce peaks after settling: output_peak={output_tail_peak} input_peak={input_tail_peak}"
        );
    }

    #[test]
    fn disabled_eq_is_bitwise_noop() {
        let mut eq = InputEqState::default();
        eq.enabled.store(false, Ordering::Relaxed);
        let original = sine_wave(1000.0, 48000.0, 256);
        let mut processed = original.clone();
        apply_eq(&mut eq, 0, processed.as_mut_ptr(), 256);
        assert_eq!(
            original, processed,
            "disabled EQ should not modify the buffer"
        );
    }

    #[test]
    fn disabled_gate_is_bitwise_noop() {
        let mut gate = InputGateState::default();
        gate.enabled.store(false, Ordering::Relaxed);
        let original = sine_wave(440.0, 48000.0, 256);
        let mut processed = original.clone();
        apply_gate(&mut gate, 0, processed.as_mut_ptr(), 256);
        assert_eq!(
            original, processed,
            "disabled gate should not modify the buffer"
        );
    }

    #[test]
    fn disabled_compressor_is_bitwise_noop() {
        let mut comp = OutputCompressorState::default();
        comp.enabled.store(false, Ordering::Relaxed);
        let original = sine_wave(440.0, 48000.0, 256);
        let mut processed = original.clone();
        apply_compressor(&mut comp, 0, processed.as_mut_ptr(), 256);
        assert_eq!(
            original, processed,
            "disabled compressor should not modify the buffer"
        );
    }

    #[test]
    fn time_constant_sanity() {
        // Typical values: attack 1-50ms, release 10-500ms at 48kHz
        for &ms in &[1.0, 5.0, 10.0, 50.0, 100.0, 500.0] {
            let c = time_constant(ms, 48000.0);
            assert!(
                c > 0.0 && c < 1.0,
                "time_constant({ms}ms) = {c}, expected in (0, 1)"
            );
        }
    }

    #[test]
    fn low_shelf_boosts_bass() {
        let sample_rate = 48000.0;
        let n = 4096;

        // Low shelf at 200Hz with +6dB gain
        let coeffs = BiquadCoeffs::low_shelf(200.0, 6.0, 0.7, sample_rate);

        // Process a low-frequency signal (100Hz)
        let low_input = sine_wave(100.0, sample_rate, n);
        let mut low_output = low_input.clone();
        let mut low_state = BiquadState::default();
        for s in &mut low_output {
            *s = low_state.process(&coeffs, *s);
        }
        // Skip transient
        let low_rms = rms(&low_output[512..]);

        // Process a high-frequency signal (4000Hz)
        let high_input = sine_wave(4000.0, sample_rate, n);
        let mut high_output = high_input.clone();
        let mut high_state = BiquadState::default();
        for s in &mut high_output {
            *s = high_state.process(&coeffs, *s);
        }
        let high_rms = rms(&high_output[512..]);

        assert!(
            low_rms > high_rms,
            "low shelf should boost bass more than treble: low_rms={low_rms} high_rms={high_rms}"
        );
    }
}
