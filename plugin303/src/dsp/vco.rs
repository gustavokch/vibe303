//! Nodal vco303 relaxation-oscillator solver (F3-VCO V3).
//!
//! Per-sample nodal MNA + Newton of the TB-303 VCO core (`circuit/vco303.toml`,
//! topology in `eval/303/_vco303_netlist_gen.add_elements`): 6 BJTs (2N3904 /
//! 2N3906), one MPF104 JFET reset switch, one 1N4148, an integrator cap C3 and
//! the comparator/output network. The integrator ramps onto C3 then the JFET
//! discharges it — a relaxation oscillator, stiff at the reset edge.
//!
//! 21 nodes; 4 are driven rails (`vp=12`, `vh=5.333`, `cvb=-4.7062+CV`, `cv=0`),
//! leaving 17 unknowns. Rails ramp 0→V over 1 ms to self-start (mirrors the SPICE
//! harness PWL). The saw output is node `o_r`, DC-blocked (the SPICE reference
//! centers it). Pitch is set by CV via the measured calibration; exact pitch /
//! waveform match to SPICE is finalized in V4/V5.
//!
//! Device models: [`super::bjt`], [`super::jfet`], [`super::diode`]. Linear solve:
//! the real-time hot path uses the fast static no-pivot
//! [`super::linalg_static::ladder_lu_solve`]; LUT warm-up / secant calibration
//! (off the audio thread) uses the robust dense [`super::linalg::lu_solve`]
//! instead. Zero-alloc hot path.

use super::bjt::{Bjt, BjtParams};
use super::diode::Diode1N4148;
use super::jfet::Jfet;
use super::linalg::lu_solve;
use super::linalg_static::vco_lu_solve;
use super::triple_buffer::TripleBuffer;
use super::worker::{submit_lut_job, LutJob};
use crate::params::OscWave;
use std::sync::{Arc, RwLock};

// --- unknown node indices ---------------------------------------------------
const II: usize = 0;
const IB: usize = 1;
const IR: usize = 2;
const CP: usize = 3;
const N4: usize = 4;
const N12: usize = 5;
const N5: usize = 6;
const N10: usize = 7;
const BF: usize = 8;
const SH_IN: usize = 9;
const N17: usize = 10;
const PSI: usize = 11;
const PL: usize = 12;
const D_R: usize = 13;
const O_R: usize = 14;
const D_P: usize = 15;
const O_P: usize = 16;
const NU: usize = 17;

/// A circuit node: either an unknown (solved) or a fixed/driven voltage.
#[derive(Clone, Copy)]
enum Node {
    Unk(usize),
    Fix(f64),
}
use Node::{Fix, Unk};

const GND: Node = Fix(0.0);

const MAX_ITERS: usize = 100;
const TOL: f64 = 1e-7;
/// Absolute tolerance for convergence (document eq. 3.30: ε_abs).
const TOL_ABS: f64 = 1e-9;
/// Relative tolerance for convergence (document eq. 3.30: ε_rel).
const TOL_REL: f64 = 1e-7;
const GMIN_BASE: f64 = 1e-12;
const RAMP_S: f64 = 1e-3; // rail ramp time for self-start
/// DC-blocker highpass corner (Hz). ~7.6 Hz reproduces the legacy hardcoded
/// `0.999` pole at 48 kHz, but is now scaled by `sr` so the corner is invariant.
const DC_BLOCK_HZ: f32 = 7.6;
/// Per-iteration node-voltage step limit (damped Newton): bounds the exponential
/// overshoot that otherwise diverges on this stiff relaxation core.
const VLIM: f64 = 0.5;
/// Minimum v[CP] upward jump (V) that is unambiguously a JFET reset.
///
/// SPICE (1.25 µs grid, cv=5.0 and 3.4): CP swings 5.35 V (bottom) → 11.20 V (top),
/// a drop of 5.85 V. Normal per-sample charging at the *highest* 303 note (C5, ~523 Hz,
/// 48 kHz, dt=20.8 µs) is ≲0.5 V. A threshold of 3.0 V sits in the noise-free gap
/// between the two regimes — no false positives, catches every JFET reset.
const CP_RESET_DELTA: f64 = 3.0;

// --- per-note pitch LUT (F6.6) ---------------------------------------------
/// Refined-CV pitch LUT (secant-calibrated against this solver's realized pitch).
/// `K` anchors span MIDI `LUT_MIDI_LO`..`+ (K-1)·LUT_MIDI_STEP`; `set_freq` does a
/// log-frequency lookup + lerp (RT-safe). The `CV_FIT` quadratic alone is ~1.4 %
/// (worse — ~6 % — at the low end), too coarse for instrument-wide pitch.
pub(crate) const LUT_K: usize = 37;
pub(crate) const LUT_MIDI_LO: f64 = 0.0; // C-1 ≈ 8.18 Hz
pub(crate) const LUT_MIDI_STEP: f64 = 3.0; // every 3 semitones → top anchor MIDI 108 (C8 ≈ 4186 Hz)

pub(crate) fn lut_midi(i: usize) -> f64 {
    LUT_MIDI_LO + i as f64 * LUT_MIDI_STEP
}
pub(crate) fn midi_hz(m: f64) -> f64 {
    440.0 * 2.0f64.powf((m - 69.0) / 12.0)
}

/// Process-global LUT cache keyed by sample rate (`f32` bits). The secant build
/// renders the VCO many times, so it must NOT run per-`Vco303`. Built once per
/// sr and reused thereafter; touched only from `Vco303::new` (reached via the
/// plugin's non-RT `initialize`, never from the audio-thread `reset`, which
/// clears the VCO in place — see `Voice::reset`). Grows by one `[f64; LUT_K]`
/// entry per *distinct* sample rate and is never freed: bounded by the handful
/// of rates a host runs in a process lifetime, so the unbounded `Vec` is a
/// non-issue (a few hundred bytes), not a leak worth a cap.
static LUT_CACHE: RwLock<Vec<(u64, [f64; LUT_K])>> = RwLock::new(Vec::new());
static DISABLE_WARM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Add a current `Σ termsⱼ·Vⱼ + constant` *leaving* `node` to the KCL system.
#[inline]
fn stamp(
    a: &mut [[f64; NU]; NU],
    rhs: &mut [f64; NU],
    node: Node,
    terms: &[(Node, f64)],
    constant: f64,
) {
    let i = match node {
        Unk(i) => i,
        Fix(_) => return, // driven node: no KCL row
    };
    for &(nref, g) in terms {
        match nref {
            Unk(j) => a[i][j] += g,
            Fix(vf) => rhs[i] -= g * vf,
        }
    }
    rhs[i] -= constant;
}

/// Conductance `g` between nodes `p` and `q` (stamps both KCL rows).
#[inline]
fn stamp_res(a: &mut [[f64; NU]; NU], rhs: &mut [f64; NU], p: Node, q: Node, g: f64) {
    stamp(a, rhs, p, &[(p, g), (q, -g)], 0.0);
    stamp(a, rhs, q, &[(q, g), (p, -g)], 0.0);
}

/// Two-terminal companion with branch current `Geq·(Vp−Vq) + ieq` leaving `p`.
#[inline]
fn stamp_branch(
    a: &mut [[f64; NU]; NU],
    rhs: &mut [f64; NU],
    p: Node,
    q: Node,
    geq: f64,
    ieq: f64,
) {
    stamp(a, rhs, p, &[(p, geq), (q, -geq)], ieq);
    stamp(a, rhs, q, &[(q, geq), (p, -geq)], -ieq);
}

#[inline]
fn vof(node: Node, v: &[f64; NU]) -> f64 {
    match node {
        Unk(i) => v[i],
        Fix(vf) => vf,
    }
}

/// Cap topology entry: the two nodes and capacitance.
struct CapDef {
    p: Node,
    q: Node,
    c: f64,
}

/// Triple-buffered background LUT store using lock-free, zero-allocation primitive.
pub struct VcoLutBuffer {
    buffer: TripleBuffer<[[f64; LUT_K]; 22]>,
    pub sequence: Arc<std::sync::atomic::AtomicU64>,
    pub completed_sequence: std::sync::atomic::AtomicU64,
}

impl VcoLutBuffer {
    pub fn new(nominal_lut: [f64; LUT_K]) -> Self {
        Self {
            buffer: TripleBuffer::new([nominal_lut; 22]),
            sequence: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            completed_sequence: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Read the currently active LUT set. Wait-free, zero-allocation, lock-free,
    /// safe to call on the real-time audio thread.
    #[inline]
    pub fn load(&self) -> &[[f64; LUT_K]; 22] {
        self.buffer.read()
    }

    /// Publish a freshly computed LUT set as the new active set.
    pub fn store(&self, luts: [[f64; LUT_K]; 22]) {
        self.buffer.write(luts);
        self.completed_sequence
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }
}

pub struct Vco303 {
    sr: f64,
    npn: BjtParams,
    pnp: BjtParams,
    diode: Diode1N4148,
    jfet: Jfet,
    /// Control voltage that sets pitch (drives `cvb = -4.7062 + cv`).
    cv: f64,
    /// Warm-start: previous converged unknown voltages.
    v: [f64; NU],
    /// Trapezoidal cap state (v_prev, i_prev) for the 5 caps.
    cvp: [f64; 5],
    cip: [f64; 5],
    r_mults: [f64; 15],
    c_mults: [f64; 5],
    g_mults: [f64; 15],
    /// Samples since note-on (for the rail self-start ramp).
    t_samples: u64,
    /// DC blocker state for the `o_r` saw output.
    dc_x1: f32,
    dc_y1: f32,
    /// DC-blocker pole, derived from `sr` so the HP corner is rate-invariant.
    dc_r: f32,
    /// Double-buffered background LUTs
    pub lut_buffer: Arc<VcoLutBuffer>,
    /// Per-note refined-CV pitch LUT (F6.6), filled from the active double-buffer
    pub cv_lut: [f64; LUT_K],
    pub wave: OscWave,
    pub sweep_pos: f64,
    instance_id: u32,
    thermal_noise: i32,
    noise_seed: u32,
    /// Diagnostics: Newton iters used + whether the last sample fell to homotopy.
    #[doc(hidden)]
    pub dbg_iters: u32,
    #[doc(hidden)]
    pub dbg_homotopy: bool,
    /// Ready-to-add BLEP correction for the **current** output sample.
    ///
    /// Set by `tick()` to the first of the two-tap BLEP residual whenever the JFET
    /// reset fires (v[CP] jumps upward by > [`CP_RESET_DELTA`] V). The caller adds
    /// this to the raw VCO output before upsampling. Zero if no reset occurred.
    ///
    /// The BLEP is calibrated to the SPICE-measured reset-step amplitude so no
    /// external scaling is needed. RT-safe; never allocates.
    pub blep_now: f32,
    /// Ready-to-add BLEP correction for the **next** output sample (the second tap).
    ///
    /// Consumed automatically at the start of the following `tick()` call and placed
    /// into `blep_now` for that sample's output. The caller should also add it if it
    /// caches VCO output for the inter-sample position.
    pub blep_next: f32,
    /// Pending second-tap BLEP value to inject at the START of the next `tick()`.
    /// Internal; carried from the reset sample to the following sample.
    blep_pending: f32,
    /// Whether a reset occurred last tick (used by `blep_frac` smoke-test compat).
    pub blep_frac: Option<f32>,

    // Synthesized square wave state
    sq_y_prev: f32,
    sq_blep_pending: f32,
    pub is_fun: bool,
    stuck_counter: u32,
    last_cp: f64,
    /// When true, `newton()` uses the robust dense partial-pivot `lu_solve`
    /// instead of the fast static `ladder_lu_solve`. Set on the throwaway
    /// instances used for LUT warm-up / secant calibration (off the audio
    /// thread, correctness matters more than speed); left `false` on
    /// real-time instances, which use the validated fast static solve.
    pub(crate) dense_solver: bool,
    loaded_sequence: u64,
}

impl Vco303 {
    const R_BASE: [f64; 15] = [
        220e3, 220e3, 4040.0, 100e3, 4700.0, 10e3, 10e3, 100e3, 10e3, 22e3, 10e3, 1e6, 220e3, 1e6,
        220e3,
    ];

    #[inline]
    fn update_g_mults(&mut self) {
        for i in 0..15 {
            self.g_mults[i] = 1.0 / (Self::R_BASE[i] * self.r_mults[i]);
        }
    }

    pub fn new(sr: f32) -> Self {
        let mut o = Self::raw(sr);
        o.cv_lut = Self::cv_lut_for(sr, 0, false); // Default to thermal_noise=0
        o.lut_buffer = Arc::new(VcoLutBuffer::new(o.cv_lut));
        o.loaded_sequence = o
            .lut_buffer
            .completed_sequence
            .load(std::sync::atomic::Ordering::Acquire);
        o
    }

    pub fn disable_warm_cache() {
        DISABLE_WARM.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Bare construct with an empty LUT — used both by `new` (before attaching
    /// the cached LUT) and by the LUT builder's throwaway render instance.
    pub(crate) fn raw(sr: f32) -> Self {
        let mut o = Self {
            sr: sr as f64,
            npn: BjtParams::n2n3904(),
            pnp: BjtParams::p2n3906(),
            diode: Diode1N4148::new(),
            jfet: Jfet::mpf104(),
            cv: 5.0,
            v: [0.0; NU],
            cvp: [0.0; 5],
            cip: [0.0; 5],
            r_mults: [1.0; 15],
            c_mults: [1.0; 5],
            g_mults: [0.0; 15],
            t_samples: 0,
            dc_x1: 0.0,
            dc_y1: 0.0,
            // ~7.6 Hz HP corner (matches the legacy 0.999 @ 48 kHz) made
            // rate-invariant: r = exp(-2π·f_hp/sr).
            dc_r: (-2.0 * std::f32::consts::PI * DC_BLOCK_HZ / sr).exp(),
            lut_buffer: std::sync::Arc::new(VcoLutBuffer::new([0.0; LUT_K])),
            cv_lut: [0.0; LUT_K],
            wave: OscWave::Saw,
            sweep_pos: 0.0,
            instance_id: 0,
            thermal_noise: 0,
            noise_seed: 12345,
            dbg_iters: 0,
            dbg_homotopy: false,
            blep_now: 0.0,
            blep_next: 0.0,
            blep_pending: 0.0,
            blep_frac: None,
            sq_y_prev: 0.0,
            sq_blep_pending: 0.0,
            is_fun: false,
            stuck_counter: 0,
            last_cp: 0.0,
            dense_solver: false,
            loaded_sequence: 0,
        };
        o.update_g_mults();
        o
    }

    /// Return the refined-CV LUT for `sr`, building (and caching) it once. The
    /// build renders a throwaway VCO per anchor (secant), so it is gated behind
    /// the process-global cache — never repeated per voice / per `reset()`.
    pub(crate) fn cv_lut_for(sr: f32, thermal_noise: i32, is_fun: bool) -> [f64; LUT_K] {
        let key = ((sr.to_bits() as u64) << 32)
            | ((thermal_noise as u64) & 0xff)
            | (if is_fun { 0x100 } else { 0x0 });
        // Fast path: check the cache under the read lock, but release it before any
        // expensive computation so other threads are not blocked during LUT build.
        {
            let cache = match LUT_CACHE.read() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(&(_, lut)) = cache.iter().find(|(k, _)| *k == key) {
                return lut;
            }
        } // read lock released here

        // Slow path: build the LUT without holding the lock.
        let mut tmp = Self::raw(sr);
        tmp.set_fun(is_fun);
        tmp.set_thermal_internal(thermal_noise);
        let mut lut = [0.0; LUT_K];
        for (i, slot) in lut.iter_mut().enumerate() {
            let target_hz = midi_hz(lut_midi(i));
            *slot = tmp.secant_cv(target_hz);
        }

        // Re-acquire write lock to insert; check again in case a concurrent thread already
        // built this key while we were computing (prefer the first to finish).
        let mut cache = match LUT_CACHE.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(&(_, existing)) = cache.iter().find(|(k, _)| *k == key) {
            // Another thread beat us to it — prefer its result (already stored).
            return existing;
        }
        cache.push((key, lut));
        lut
    }

    /// Pre-warm the process-global LUT cache for step 0 (nominal temperature) only.
    /// Fast nominal warming for unit tests to run in sub-second time.
    pub fn warm_cache_nominal(sr: f32, is_fun: bool) {
        Self::cv_lut_for(sr, 0, is_fun);
    }

    /// Pre-warm the process-global LUT cache for all 22 thermal-noise steps at
    /// the given `sr` and `is_fun` combination. After this returns, every
    /// `recalculate_luts_async` call with `age=0, tolerance=0.0` will hit the
    /// fast-path (pure cache lookups, ~2 µs, no background thread). Expensive
    /// on first call for a given `sr`, but idempotent — subsequent calls
    /// are instant because `cv_lut_for` checks the cache first.
    pub fn warm_cache(sr: f32, is_fun: bool) {
        for step in 0..22i32 {
            Self::cv_lut_for(sr, step, is_fun);
        }
    }

    /// Secant-solve the CV that makes this solver realize `target` Hz. Seeded
    /// from the `CV_FIT` quadratic, refined to ≲0.1 %. Runs only at LUT build.
    pub(crate) fn secant_cv(&mut self, target: f64) -> f64 {
        self.secant_cv_with_hint(target, None)
    }

    pub(crate) fn secant_cv_with_hint(&mut self, target: f64, hint: Option<f64>) -> f64 {
        let cv_seed = hint.unwrap_or_else(|| cv_for_freq(target));
        let mut cv0 = cv_seed;
        let mut f0 = self.measure_freq(cv0);
        if f0 <= 0.0 {
            return cv_seed;
        }
        let mut best_cv = cv0;
        let mut best_err = (f0 / target - 1.0).abs();
        if best_err < 1e-3 {
            return cv0;
        }

        let mut cv1 = cv0 + 0.02;
        let mut f1 = self.measure_freq(cv1);
        if f1 <= 0.0 {
            return cv_seed;
        }
        let err1 = (f1 / target - 1.0).abs();
        if err1 < best_err {
            best_err = err1;
            best_cv = cv1;
        }

        for _ in 0..8 {
            let denom = f1 - f0;
            if denom.abs() < 1e-9 {
                break;
            }
            let cv2 = (cv1 - (f1 - target) * (cv1 - cv0) / denom).clamp(cv_seed - 2.0, cv_seed + 2.0);
            cv0 = cv1;
            f0 = f1;
            cv1 = cv2;
            f1 = self.measure_freq(cv1);
            if f1 <= 0.0 {
                break;
            }
            let err = (f1 / target - 1.0).abs();
            if err < best_err {
                best_err = err;
                best_cv = cv1;
            }
            if err < 1e-3 {
                return cv1;
            }
        }
        best_cv
    }

    /// Realized fundamental (Hz) at a fixed `cv`, via interpolated downward
    /// threshold crossings of the integrator capacitor node CP over a steady window.
    /// Mutates state; LUT-build only.
    fn measure_freq(&mut self, cv: f64) -> f64 {
        self.reset();
        self.cv = cv;
        let log2_f = ((cv + 3.8) / 0.84).max(1.0);
        let f_guess = 2.0f64.powf(log2_f);
        let period_samples = self.sr / f_guess.max(1.0);

        let warm = (period_samples * 12.0).max(self.sr * 0.020) as usize;
        for _ in 0..warm {
            self.tick();
        }
        let n = (period_samples * 16.0).max(self.sr * 0.010) as usize;
        let mut prev_cp = self.v[CP];
        let (mut first_t, mut last_t, mut count) = (-1.0f64, 0.0f64, 0i64);
        const THRESH: f64 = 6.0;
        for i in 0..n {
            self.tick();
            let cp = self.v[CP];
            if prev_cp >= THRESH && cp < THRESH {
                let frac = (prev_cp - THRESH) / (prev_cp - cp);
                let t = i as f64 + frac;
                if first_t < 0.0 {
                    first_t = t;
                } else {
                    last_t = t;
                    count += 1;
                }
            }
            prev_cp = cp;
        }
        if count == 0 || last_t <= first_t {
            return 0.0;
        }
        count as f64 * self.sr / (last_t - first_t)
    }

    /// Raw converged node voltages (diagnostics).
    pub fn node(&self, i: usize) -> f64 {
        self.v[i]
    }

    /// Set pitch from the refined-CV LUT (F6.6): log-frequency lookup + linear
    /// interpolation between anchors. RT-safe — no render, no alloc. Outside the
    /// LUT span it clamps to the end anchors (the quadratic `CV_FIT` seed still
    /// underlies the build, so the edges degrade gracefully).
    pub fn set_freq(&mut self, hz: f32) {
        let f = (hz as f64).max(1.0);
        // Anchor frequencies are log-uniform (equal MIDI steps), so position in
        // semitones maps linearly to the LUT index.
        let midi = 69.0 + 12.0 * (f / 440.0).log2();
        let pos = (midi - LUT_MIDI_LO) / LUT_MIDI_STEP;
        let pos = pos.clamp(0.0, (LUT_K - 1) as f64);
        let i = pos.floor() as usize;
        self.cv = if i >= LUT_K - 1 {
            self.cv_lut[LUT_K - 1]
        } else {
            let frac = pos - i as f64;
            self.cv_lut[i] * (1.0 - frac) + self.cv_lut[i + 1] * frac
        };
    }

    /// Raw control voltage (diagnostics / calibration).
    pub fn set_cv(&mut self, cv: f64) {
        self.cv = cv;
    }

    pub fn set_analog_wear(&mut self, tolerance: f64, age: i32, seed: u32) {
        let mut rng_state = seed.wrapping_add(self.instance_id.wrapping_mul(0x9E3779B9));
        let mut next_f64 = || {
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            (rng_state as f64) / (std::u32::MAX as f64)
        };
        let mut next_gauss = || {
            let mut u1 = next_f64();
            if u1 < 1e-6 {
                u1 = 1e-6;
            }
            let u2 = next_f64();
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        };

        let systematic_r = 0.05 * (age as f64 / 100.0);
        let systematic_c = -0.05 * (age as f64 / 100.0);

        let bjt_age_factor = 1.0 - 0.15 * (age as f64 / 100.0);
        self.npn.bf = BjtParams::n2n3904().bf * bjt_age_factor;
        self.npn.br = BjtParams::n2n3904().br * bjt_age_factor;
        self.pnp.bf = BjtParams::p2n3906().bf * bjt_age_factor;
        self.pnp.br = BjtParams::p2n3906().br * bjt_age_factor;

        let sigma = tolerance / 3.0;
        for m in self.r_mults.iter_mut() {
            let r_dev = next_gauss() * sigma;
            *m = (1.0 + systematic_r + r_dev).clamp(0.75, 1.25);
        }
        for m in self.c_mults.iter_mut() {
            let c_dev = next_gauss() * sigma;
            *m = (1.0 + systematic_c + c_dev).clamp(0.75, 1.25);
        }
        self.update_g_mults();
    }

    pub fn set_thermal_internal(&mut self, thermal_noise: i32) {
        let temp_c = 25.0 + thermal_noise as f64;
        self.npn.set_temp(temp_c);
        self.pnp.set_temp(temp_c);
        self.diode.set_temp(temp_c);
    }

    pub fn set_thermal(&mut self, thermal_noise: i32) {
        self.thermal_noise = thermal_noise;
        self.set_thermal_internal(thermal_noise);
        let noise_idx = thermal_noise.clamp(0, 21) as usize;
        self.cv_lut = self.lut_buffer.load()[noise_idx];
    }

    pub fn poll_lut_update(&mut self) {
        let comp_seq = self
            .lut_buffer
            .completed_sequence
            .load(std::sync::atomic::Ordering::Acquire);
        if comp_seq != self.loaded_sequence {
            self.loaded_sequence = comp_seq;
            let noise_idx = self.thermal_noise.clamp(0, 21) as usize;
            self.cv_lut = self.lut_buffer.load()[noise_idx];
        }
    }

    // Public for integration tests only
    pub fn build_step0_sync(&mut self) {
        let lut = Self::cv_lut_for(self.sr as f32, 0, self.is_fun);
        self.lut_buffer.store([lut; 22]);
        self.cv_lut = lut;
    }

    pub fn get_cv_lut(&self) -> [f64; LUT_K] {
        self.cv_lut
    }

    pub fn solve_cv_lut_with_recal(&mut self, recal_scale: f64) -> [f64; LUT_K] {
        let nominal_lut = if recal_scale < 1.0 {
            Some(Self::cv_lut_for(self.sr as f32, 0, self.is_fun))
        } else {
            None
        };
        let mut lut = [0.0f64; LUT_K];
        for i in 0..LUT_K {
            let f_nom = midi_hz(lut_midi(i));
            if f_nom < 18.0 {
                lut[i] = cv_for_freq(f_nom);
            } else if recal_scale >= 1.0 || nominal_lut.is_none() {
                let cv_sol = self.secant_cv_with_hint(f_nom, None);
                lut[i] = if cv_sol.is_finite() { cv_sol } else { cv_for_freq(f_nom) };
            } else {
                let cv_nom = nominal_lut.unwrap()[i];
                let f_raw = self.measure_freq(cv_nom);
                if f_raw <= 0.0 {
                    lut[i] = self.secant_cv_with_hint(f_nom, Some(cv_nom));
                } else {
                    let delta_p_uncal = 1200.0 * (f_raw / f_nom).log2();
                    let delta_p_eff = (delta_p_uncal * (1.0 - recal_scale)).clamp(-30.0, 30.0);
                    let f_target = f_nom * 2.0f64.powf(delta_p_eff / 1200.0);
                    lut[i] = self.secant_cv_with_hint(f_target, Some(cv_nom));
                }
            }
        }
        // Post-loop validation pass for interior anchors
        for i in 1..LUT_K - 1 {
            let f_nom = midi_hz(lut_midi(i));
            if !lut[i].is_finite() {
                let prev = (0..i).rev().find(|&j| lut[j].is_finite()).unwrap_or(0);
                let next = (i + 1..LUT_K).find(|&j| lut[j].is_finite()).unwrap_or(LUT_K - 1);
                let denom = (next - prev) as f64;
                if denom > 0.0 {
                    let t = (i - prev) as f64 / denom;
                    lut[i] = lut[prev] + t * (lut[next] - lut[prev]);
                } else {
                    lut[i] = cv_for_freq(f_nom);
                }
            }
        }
        // Fallback to linear extrapolation near boundary anchors (i=0 or i=LUT_K-1) if unconverged or non-finite
        if !lut[0].is_finite() {
            if lut[1].is_finite() && lut[2].is_finite() {
                let step = lut[2] - lut[1];
                lut[0] = lut[1] - step;
            } else {
                lut[0] = cv_for_freq(midi_hz(lut_midi(0)));
            }
        }
        let last = LUT_K - 1;
        if !lut[last].is_finite() {
            if lut[last - 1].is_finite() && lut[last - 2].is_finite() {
                let step = lut[last - 1] - lut[last - 2];
                lut[last] = lut[last - 1] + step;
            } else {
                lut[last] = cv_for_freq(midi_hz(lut_midi(last)));
            }
        }
        lut
    }

    pub fn recalculate_luts_async(&self, age: i32, tolerance: f64, seed: u32, recal_scale: f64) -> bool {
        let is_fun = self.is_fun;
        if age == 0 && tolerance == 0.0 && recal_scale == 1.0 {
            let sr = self.sr as f32;
            let step0 = Self::cv_lut_for(sr, 0, is_fun);
            let mut local_luts = [step0; 22];
            let cache = match LUT_CACHE.read() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            for noise_step in 1..22i32 {
                let key = ((sr.to_bits() as u64) << 32)
                    | ((noise_step as u64) & 0xff)
                    | (if is_fun { 0x100 } else { 0x0 });
                if let Some(&(_, lut)) = cache.iter().find(|(k, _)| *k == key) {
                    local_luts[noise_step as usize] = lut;
                }
            }
            self.lut_buffer.sequence.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.lut_buffer.store(local_luts);
            return true;
        }

        let lut_buffer = Arc::clone(&self.lut_buffer);
        let seq = lut_buffer
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;

        let seq_checker = Arc::clone(&lut_buffer.sequence);
        let buffer_for_complete = Arc::clone(&lut_buffer);

        let job = LutJob {
            sr: self.sr as f32,
            instance_id: self.instance_id,
            is_fun,
            age,
            tolerance,
            recal_scale,
            seed,
            sequence: seq,
            sequence_checker: seq_checker,
            on_complete: Arc::new(move |luts| {
                buffer_for_complete.store(luts);
            }),
        };

        submit_lut_job(job)
    }

    pub fn recalculate_luts_sync_with_recal(
        &mut self,
        age: i32,
        tolerance: f64,
        seed: u32,
        recal_scale: f64,
    ) {
        self.lut_buffer
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let is_fun = self.is_fun;
        if age == 0 && tolerance == 0.0 && recal_scale == 1.0 {
            let lut = Self::cv_lut_for(self.sr as f32, 0, is_fun);
            self.lut_buffer.store([lut; 22]);
            self.loaded_sequence = self
                .lut_buffer
                .completed_sequence
                .load(std::sync::atomic::Ordering::Acquire);
            self.cv_lut = lut;
            return;
        }

        self.set_analog_wear(tolerance, age, seed);

        let mut local_luts = [[0.0f64; LUT_K]; 22];
        self.set_thermal_internal(0);
        let lut_lo = self.solve_cv_lut_with_recal(recal_scale);
        local_luts[0] = lut_lo;

        self.set_thermal_internal(21);
        let lut_hi = self.solve_cv_lut_with_recal(recal_scale);
        local_luts[21] = lut_hi;

        for step in 1..21 {
            let t = step as f64 / 21.0;
            for i in 0..LUT_K {
                local_luts[step][i] = lut_lo[i] + t * (lut_hi[i] - lut_lo[i]);
            }
        }

        self.lut_buffer.store(local_luts);
        self.loaded_sequence = self
            .lut_buffer
            .completed_sequence
            .load(std::sync::atomic::Ordering::Acquire);
        // The endpoint solves left the device models at the step-21 temperature;
        // restore them to the VCO's active thermal step so the live solver matches
        // the selected cv_lut.
        self.set_thermal_internal(self.thermal_noise);
        let noise_idx = self.thermal_noise.clamp(0, 21) as usize;
        self.cv_lut = local_luts[noise_idx];
        self.reset();
    }

    pub fn recalculate_luts_sync(&mut self, age: i32, tolerance: f64) {
        self.recalculate_luts_sync_with_recal(age, tolerance, 1337, 1.0);
    }

    pub fn set_fun(&mut self, is_fun: bool) {
        if self.is_fun != is_fun {
            self.is_fun = is_fun;
            let mut jfet = Jfet::mpf104();
            if is_fun {
                jfet.beta *= 1.5;
            }
            self.jfet = jfet;
        }
    }

    pub fn cv(&self) -> f64 {
        self.cv
    }

    pub fn t_samples(&self) -> u64 {
        self.t_samples
    }

    pub fn set_wave(&mut self, wave: OscWave) {
        self.wave = wave;
    }

    pub fn set_sweep(&mut self, pos: f64) {
        self.sweep_pos = pos;
    }

    pub fn set_instance_id(&mut self, id: u32) {
        self.instance_id = id;
        self.noise_seed = 12345u32.wrapping_add(id * 1000);
    }

    pub fn get_c_mults(&self) -> [f64; 5] {
        self.c_mults
    }

    pub fn get_r_mults(&self) -> [f64; 15] {
        self.r_mults
    }

    pub fn reset(&mut self) {
        self.v = [0.0; NU];
        self.v[CP] = 12.0; // numerical kick
        self.v[PSI] = 5.333; // pre-charge C2 DC bias node PSI to VH
        self.cvp = [0.0; 5];
        self.cip = [0.0; 5];
        self.t_samples = 0;
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
        self.blep_now = 0.0;
        self.blep_next = 0.0;
        self.blep_pending = 0.0;
        self.blep_frac = None;
        self.stuck_counter = 0;
        self.last_cp = 0.0;
    }

    /// Driven rail voltages at the current sample (ramped 0→V over `RAMP_S`).
    fn rails(&self) -> (f64, f64, f64, f64) {
        let t = self.t_samples as f64 / self.sr;
        let k = (t / RAMP_S).min(1.0);
        let vp = 12.0 * k;
        let vh = 5.333 * k;
        let cvb = (-4.7062 + self.cv) * k;
        let cv = 0.0;
        (vp, vh, cvb, cv)
    }

    /// Cap definitions for this sample's rail values.
    fn caps(&self, vp: f64) -> [CapDef; 5] {
        [
            CapDef {
                p: Fix(vp),
                q: Unk(CP),
                c: 1e-8 * self.c_mults[0],
            }, // C3 integrator
            CapDef {
                p: Unk(BF),
                q: Unk(N17),
                c: 1e-8 * self.c_mults[1],
            }, // C1
            CapDef {
                p: Unk(PSI),
                q: GND,
                c: 1e-6 * self.c_mults[2],
            }, // C2
            CapDef {
                p: Unk(D_R),
                q: Unk(O_R),
                c: 1e-7 * self.c_mults[3],
            }, // C90
            CapDef {
                p: Unk(D_P),
                q: Unk(O_P),
                c: 1e-7 * self.c_mults[4],
            }, // C80
        ]
    }

    /// Assemble + stamp the linear elements of the system: resistors, capacitor companions,
    /// and gmin/base leakage on the diagonal.
    #[inline]
    fn assemble_linear(
        &self,
        rails: (f64, f64, f64, f64),
        cgeq: &[f64; 5],
        cieq: &[f64; 5],
        gmin: f64,
        i_noise: f64,
    ) -> ([[f64; NU]; NU], [f64; NU]) {
        let (vp, vh, cvb, cv) = rails;
        let mut a = [[0.0f64; NU]; NU];
        let mut rhs = [0.0f64; NU];

        // Resistors.
        stamp_res(&mut a, &mut rhs, Fix(cvb), Unk(II), self.g_mults[0]); // R77
        stamp_res(&mut a, &mut rhs, Fix(cv), Unk(II), self.g_mults[1]); // R7
        stamp_res(&mut a, &mut rhs, Unk(II), GND, self.g_mults[2]); // R8
        stamp_res(&mut a, &mut rhs, Fix(vp), Unk(IB), self.g_mults[3]); // R9
        stamp_res(&mut a, &mut rhs, Unk(CP), Unk(IR), self.g_mults[4]); // R88
        stamp_res(&mut a, &mut rhs, Unk(N10), Fix(vh), self.g_mults[5]); // R6
        stamp_res(&mut a, &mut rhs, Unk(BF), GND, self.g_mults[6]); // R1
        stamp_res(&mut a, &mut rhs, Unk(BF), Unk(SH_IN), self.g_mults[7]); // R2
        stamp_res(&mut a, &mut rhs, Unk(N17), Unk(SH_IN), self.g_mults[8]); // R3
        stamp_res(&mut a, &mut rhs, Fix(vp), Unk(PSI), self.g_mults[9]); // R5
        stamp_res(&mut a, &mut rhs, Fix(vh), Unk(PL), self.g_mults[10]); // R4
        stamp_res(&mut a, &mut rhs, Unk(BF), Unk(D_R), self.g_mults[11]); // R90
        stamp_res(&mut a, &mut rhs, Unk(O_R), GND, self.g_mults[12]); // R91
        stamp_res(&mut a, &mut rhs, Unk(PL), Unk(D_P), self.g_mults[13]); // R80
        stamp_res(&mut a, &mut rhs, Unk(O_P), GND, self.g_mults[14]); // R81

        // Cap companions.
        let cdefs = self.caps(vp);
        for (j, cd) in cdefs.iter().enumerate() {
            // Trapezoidal: branch current leaving p = Geq·(Vp−Vq) − Ieq.
            stamp_branch(&mut a, &mut rhs, cd.p, cd.q, cgeq[j], -cieq[j]);
        }

        // Inject physical noise current into integrator capacitor node CP
        stamp(&mut a, &mut rhs, Unk(CP), &[], i_noise);

        // gmin + base leakage on every diagonal.
        for d in 0..NU {
            a[d][d] += gmin + GMIN_BASE;
        }
        (a, rhs)
    }

    /// Add the stamps for nonlinear elements (diodes, BJTs, JFETs) to the pre-assembled system.
    #[inline]
    fn add_nonlinear(&self, a: &mut [[f64; NU]; NU], rhs: &mut [f64; NU], v: &[f64; NU], vp: f64) {
        // Diode D1: anode n4 -> cathode n5.
        {
            let vd = vof(Unk(N4), v) - vof(Unk(N5), v);
            let (id, gd) = self.diode.stamp(vd);
            let ieq = id - gd * vd;
            stamp_branch(a, rhs, Unk(N4), Unk(N5), gd, ieq);
        }

        // BJTs: (device, base, collector, emitter).
        let bjts: [(Bjt, Node, Node, Node); 6] = [
            (Bjt::pnp(self.pnp), Unk(II), GND, Unk(IB)),         // Q6
            (Bjt::npn(self.npn), Unk(IB), Unk(IR), GND),         // Q7
            (Bjt::pnp(self.pnp), Unk(N12), Unk(N4), Fix(vp)),    // Q4
            (Bjt::npn(self.npn), Unk(N5), Unk(N5), Unk(N10)),    // Q5 (diode-connected)
            (Bjt::npn(self.npn), Unk(N5), Unk(N12), Unk(CP)),    // Q3
            (Bjt::pnp(self.pnp), Unk(SH_IN), Unk(PL), Unk(PSI)), // Q2
        ];
        for (dev, nb, nc, ne) in bjts {
            let vb = vof(nb, v);
            let vc = vof(nc, v);
            let ve = vof(ne, v);
            let o = dev.solve(vb - ve, vb - vc);
            // Each terminal current (into device) leaving its node, linearized in
            // the node voltages; constant = I0 - Σ g·V0.
            let i_eq_b = o.ib - (o.dib_b * vb + o.dib_c * vc + o.dib_e * ve);
            let i_eq_c = o.ic - (o.dic_b * vb + o.dic_c * vc + o.dic_e * ve);
            let i_eq_e = o.ie - (o.die_b * vb + o.die_c * vc + o.die_e * ve);
            stamp(
                a,
                rhs,
                nb,
                &[(nb, o.dib_b), (nc, o.dib_c), (ne, o.dib_e)],
                i_eq_b,
            );
            stamp(
                a,
                rhs,
                nc,
                &[(nb, o.dic_b), (nc, o.dic_c), (ne, o.dic_e)],
                i_eq_c,
            );
            stamp(
                a,
                rhs,
                ne,
                &[(nb, o.die_b), (nc, o.die_c), (ne, o.die_e)],
                i_eq_e,
            );
        }

        // JFET Q1: drain=vp, gate=cp, source=bf.
        {
            let nd = Fix(vp);
            let ng = Unk(CP);
            let ns = Unk(BF);
            let vd = vof(nd, v);
            let vg = vof(ng, v);
            let vs = vof(ns, v);
            let vgs = vg - vs;
            let vds = vd - vs;
            let ch = self.jfet.channel(vgs, vds);
            // Drain current leaving drain node: gm·Vg + gds·Vd − (gm+gds)·Vs + const.
            let gm = ch.gm;
            let gds = ch.gds;
            let i_eq = ch.id - (gm * vgs + gds * vds);
            stamp(a, rhs, nd, &[(ng, gm), (nd, gds), (ns, -(gm + gds))], i_eq);
            stamp(a, rhs, ns, &[(ng, -gm), (nd, -gds), (ns, gm + gds)], -i_eq);
            // Gate junction diodes: gate->source, gate->drain.
            let vgsj = vg - vs;
            let (igs, ggs) = self.jfet.gate_diode(vgsj);
            stamp_branch(a, rhs, ng, ns, ggs, igs - ggs * vgsj);
            let vgdj = vg - vd;
            let (igd, ggd) = self.jfet.gate_diode(vgdj);
            stamp_branch(a, rhs, ng, nd, ggd, igd - ggd * vgdj);
        }
    }

    /// One Newton solve at precomputed base system, warm-started from `v0`.
    fn newton(
        &self,
        base_a: &[[f64; NU]; NU],
        base_rhs: &[f64; NU],
        v0: [f64; NU],
        vp: f64,
    ) -> Option<([f64; NU], u32)> {
        let mut v = v0;
        for it in 0..MAX_ITERS {
            let mut a = *base_a;
            let mut rhs = *base_rhs;
            self.add_nonlinear(&mut a, &mut rhs, &v, vp);
            // Off the audio thread (LUT warm-up / secant calibration,
            // `self.dense_solver`) we use the robust dense partial-pivot
            // `lu_solve`. The real-time hot path instead uses the fast static
            // no-pivot ladder LU (validated vs dense in
            // `vco_static_lu_matches_dense`, rel err ≪ TOL; ~10× fewer ops).
            // NaN from a singular diagonal, or a failed dense solve, is caught
            // below → drops to the gmin homotopy.
            let sol = if self.dense_solver {
                match lu_solve::<NU>(a, rhs) {
                    Some(s) => s,
                    None => return None,
                }
            } else {
                let s = vco_lu_solve(a, rhs);
                if s.iter().all(|x| x.is_finite()) {
                    s
                } else {
                    match lu_solve::<NU>(a, rhs) {
                        Some(s) => s,
                        None => return None,
                    }
                }
            };
            let mut dmax = 0.0f64;
            for d in 0..NU {
                if !sol[d].is_finite() {
                    return None;
                }
                // Damped step: bound |Δv| per node to VLIM.
                let step = (sol[d] - v[d]).clamp(-VLIM, VLIM);
                v[d] += step;
                let dd = step.abs();
                if dd > dmax {
                    dmax = dd;
                }
            }
            if dmax < TOL {
                return Some((v, it as u32 + 1));
            }
            // Mixed absolute + relative tolerance (document eq. 3.30)
            // ε_abs + ε_rel × |v_max| handles both small and large signal swings
            let vmax = v.iter().fold(0.0f64, |a, &x| a.abs().max(x.abs()));
            if dmax < TOL_ABS + TOL_REL * vmax {
                return Some((v, it as u32 + 1));
            }
        }
        None
    }

    /// Solve one sample; returns the DC-blocked `o_r` saw + accumulated BLEP residuals.
    ///
    /// The caller must add `self.blep_now` (set by this call) to the returned raw value
    /// to get the band-limited output. `blep_now` is zero when no reset occurred.
    pub fn tick(&mut self) -> f32 {
        // --- JFET Perturbation Watchdog ---
        // If the integrator capacitor node CP remains completely static for over 2000 samples
        // (potential numerical latch-up in the relaxation loop), inject a minor perturbation
        // on CP based on sample count to kick-start oscillation without causing audible discontinuities.
        let cp_val = self.v[CP];
        if (cp_val - self.last_cp).abs() < 1e-12 {
            self.stuck_counter += 1;
            if self.stuck_counter > 2000 {
                self.v[CP] = 12.0 - 0.1 * (self.t_samples % 7) as f64;
                self.stuck_counter = 0;
            }
        } else {
            self.stuck_counter = 0;
        }
        self.last_cp = cp_val;

        // --- Carry the pending 2nd-tap BLEP from last sample's reset --------
        // The 2-tap BLEP must modify TWO adjacent samples: the reset sample itself
        // (tap 0 = blep_now) and the FOLLOWING sample (tap 1 = blep_pending → blep_now
        // next tick). blep_next is read by the caller if it needs lookahead; here we
        // just forward blep_pending into blep_now for this sample.
        self.blep_now = self.blep_pending;
        self.blep_next = 0.0;
        self.blep_pending = 0.0;
        self.blep_frac = None; // compat field

        let dt = 1.0 / self.sr;
        let rails = self.rails();
        let vp = rails.0;

        // Save cp before the solve — needed for reset detection.
        let cp_prev = self.v[CP];

        let cdefs = self.caps(vp);
        let mut cgeq = [0.0f64; 5];
        let mut cieq = [0.0f64; 5];
        for (j, cd) in cdefs.iter().enumerate() {
            let geq = 2.0 * cd.c / dt;
            cgeq[j] = geq;
            cieq[j] = geq * self.cvp[j] + self.cip[j];
        }

        let mut i_noise = 0.0;
        if self.thermal_noise > 0 {
            self.noise_seed = self.noise_seed.wrapping_mul(1103515245).wrapping_add(12345);
            let white = ((self.noise_seed as f64) / (std::u32::MAX as f64)) * 2.0 - 1.0;
            i_noise = white * 5e-8 * (self.thermal_noise as f64 / 21.0);
        }

        // Fast path: warm-started Newton at gmin=0; else descend a homotopy.
        let (base_a_0, base_rhs_0) = self.assemble_linear(rails, &cgeq, &cieq, 0.0, i_noise);
        let v = match self.newton(&base_a_0, &base_rhs_0, self.v, vp) {
            Some((v, it)) => {
                self.dbg_iters = it;
                self.dbg_homotopy = false;
                v
            }
            None => {
                self.dbg_homotopy = true;
                let mut vh = self.v;
                let mut its = 0;
                let mut converged0 = false;
                for &gmin in &[1e-1, 1e-2, 1e-3, 1e-4, 0.0] {
                    let (base_a, base_rhs) =
                        self.assemble_linear(rails, &cgeq, &cieq, gmin, i_noise);
                    match self.newton(&base_a, &base_rhs, vh, vp) {
                        Some((s, it)) => {
                            vh = s;
                            its += it;
                            // Only the final gmin=0 step is a physical operating
                            // point; the intermediate steps carry artificial
                            // leakage (≥1e-4 S ≈ R8 conductance) that would emit a
                            // materially wrong sample.
                            converged0 = gmin == 0.0;
                        }
                        None => break,
                    }
                }
                self.dbg_iters = its;
                if converged0 {
                    vh
                } else {
                    // Homotopy never reached gmin=0: hold the previous converged
                    // operating point rather than emit a leakage-contaminated one.
                    self.v
                }
            }
        };
        self.v = v;

        // --- JFET reset detection + 2-tap BLEP computation ------------------
        let cp_now = v[CP];
        let delta = cp_now - cp_prev;

        let mut sq_blep0 = self.sq_blep_pending;
        self.sq_blep_pending = 0.0;

        if delta > CP_RESET_DELTA {
            let mid = (cp_prev + cp_now) * 0.5;
            let alpha = ((mid - cp_prev) / delta.max(1e-9)).clamp(0.0, 1.0) as f32;
            const BLEP_STEP: f32 = 2.92; // OR_SWING (1.46 V) × VCO_GAIN (2.0)
            let tap0 = BLEP_STEP * (alpha * alpha / 2.0 - alpha); // < 0 for α∈(0,1)
            let tap1 = BLEP_STEP * (-alpha * alpha / 2.0); // ≤ 0
            self.blep_now += tap0;
            self.blep_pending = tap1;
            self.blep_next = tap1;
            self.blep_frac = Some(alpha);

            // Square wave downward step at JFET reset (matching saw)
            // We're synthesizing a pulse that swings -0.56 to +0.56 (p-p 1.12V)
            // VCO_GAIN (2.607) brings this to 2.92V p-p.
            // Step size is -1.12V (downward)
            const SQ_DOWN_STEP: f32 = -1.12;
            sq_blep0 += SQ_DOWN_STEP * (alpha * alpha / 2.0 - alpha);
            self.sq_blep_pending += SQ_DOWN_STEP * (-alpha * alpha / 2.0);
        }
        // --- end BLEP ---------------------------------------------------

        for (j, cd) in cdefs.iter().enumerate() {
            let vc = vof(cd.p, &v) - vof(cd.q, &v);
            let inew = cgeq[j] * vc - cieq[j];
            self.cvp[j] = if vc.abs() < 1e-30 { 0.0 } else { vc };
            self.cip[j] = if inew.abs() < 1e-30 { 0.0 } else { inew };
        }
        self.t_samples += 1;

        let saw_raw = v[O_R] as f32;
        let y_saw = saw_raw - self.dc_x1 + self.dc_r * self.dc_y1;
        self.dc_x1 = saw_raw;
        self.dc_y1 = y_saw;

        // Upward zero crossing detection (smooth ramp crossing zero)
        if self.sq_y_prev <= 0.0 && y_saw > 0.0 {
            // alpha is the fraction of the sample BEFORE the crossing
            let alpha = (self.sq_y_prev / (self.sq_y_prev - y_saw)).clamp(0.0, 1.0);
            const SQ_UP_STEP: f32 = 1.12;
            sq_blep0 += SQ_UP_STEP * (alpha * alpha / 2.0 - alpha);
            self.sq_blep_pending += SQ_UP_STEP * (-alpha * alpha / 2.0);
        }
        self.sq_y_prev = y_saw;

        // Synthesize the square wave (±0.56V) + BLEP
        let pulse_raw = if y_saw > 0.0 { 0.56 } else { -0.56 } + sq_blep0;

        match self.wave {
            OscWave::Saw => y_saw,
            OscWave::Square => pulse_raw,
            OscWave::Sweep => {
                y_saw * (1.0 - self.sweep_pos as f32) + pulse_raw * self.sweep_pos as f32
            }
        }
    }
}

/// CV for a target pitch: quadratic fit of CV in `log2(freq)` calibrated to **this
/// solver's** measured `freq(cv)` (not SPICE's — our reset timing differs ~1.4×).
/// Good to ~1.4 % pitch; a per-note secant refinement for tighter match is V5.
pub fn cv_for_freq(freq: f64) -> f64 {
    const A: f64 = CV_FIT[0];
    const B: f64 = CV_FIT[1];
    const C: f64 = CV_FIT[2];
    let l = freq.max(1.0).log2();
    A * l * l + B * l + C
}

/// Quadratic coefficients [a, b, c] for cv = a·x² + b·x + c, x = log2(freq).
/// Refit offline from `characterize_freq_cv` (this solver's measured points).
const CV_FIT: [f64; 3] = [0.011_078_504_3, 0.844_571_322_2, -3.808_754_753_1];

#[cfg(test)]
impl Vco303 {
    /// Test hook: assemble the gmin=0 system at the current warm state (mirrors
    /// `tick`'s companion setup). Feeds the structure/codegen analysis real
    /// runtime matrices.
    fn capture_system(&self) -> ([[f64; NU]; NU], [f64; NU]) {
        let dt = 1.0 / self.sr;
        let rails = self.rails();
        let vp = rails.0;
        let cdefs = self.caps(vp);
        let mut cgeq = [0.0f64; 5];
        let mut cieq = [0.0f64; 5];
        for (j, cd) in cdefs.iter().enumerate() {
            let geq = 2.0 * cd.c / dt;
            cgeq[j] = geq;
            cieq[j] = geq * self.cvp[j] + self.cip[j];
        }
        let (mut a, mut rhs) = self.assemble_linear(rails, &cgeq, &cieq, 0.0, 0.0);
        self.add_nonlinear(&mut a, &mut rhs, &self.v, vp);
        (a, rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_g_mults_updated() {
        let mut vco = Vco303::new(48000.0);
        let initial_g = vco.g_mults[0];
        vco.set_analog_wear(0.05, 5, 1337);
        assert_ne!(vco.g_mults[0], initial_g);
    }

    #[test]
    fn test_assemble_linear_exists() {
        let vco = Vco303::new(48000.0);
        let (a, _rhs) =
            vco.assemble_linear((12.0, 5.333, 0.0, 0.0), &[0.0; 5], &[0.0; 5], 0.0, 0.0);
        assert!(a[0][0] > 0.0);
    }

    /// Capture VCO Jacobians over several pitches, build the static fill pattern,
    /// check no-pivot safety (natural-order vs dense partial-pivot), and emit the
    /// straight-line static LU to scratchpad for paste into `linalg_static`.
    #[test]
    #[ignore = "codegen: emit + check static sparse LU for the VCO 17×17 pattern"]
    fn emit_and_check_vco_lu() {
        let sr = 48_000.0f32;
        let mut pattern = [[false; NU]; NU];
        let mut worst_err = 0.0f64;
        let mut worst_growth = 0.0f64;
        // Sweep CVs (pitches) + capture across the relaxation cycle (incl. the
        // stiff reset edge) so the pattern is the true topological union.
        for &cv in &[2.5f64, 3.5, 4.5, 5.5, 6.5] {
            let mut o = Vco303::raw(sr);
            o.set_cv(cv);
            for i in 0..4000 {
                o.tick();
                if i >= 1000 && i % 7 == 0 {
                    let (a, b) = o.capture_system();
                    for r in 0..NU {
                        for c in 0..NU {
                            if a[r][c] != 0.0 {
                                pattern[r][c] = true;
                            }
                        }
                    }
                    // No-pivot vs dense check.
                    if let Some(dense) = lu_solve::<NU>(a, b) {
                        let (nat, gr) = natural_solve(&a, &b);
                        let err = (0..NU)
                            .map(|k| (nat[k] - dense[k]).abs() / dense[k].abs().max(1e-9))
                            .fold(0.0, f64::max);
                        worst_err = worst_err.max(err);
                        worst_growth = worst_growth.max(gr);
                    }
                }
            }
        }
        let mut fp = pattern;
        for col in 0..NU {
            for r in (col + 1)..NU {
                if fp[r][col] {
                    for c in (col + 1)..NU {
                        if fp[col][c] {
                            fp[r][c] = true;
                        }
                    }
                }
            }
        }
        let nnz: usize = pattern.iter().flatten().filter(|&&b| b).count();
        let fnnz: usize = fp.iter().flatten().filter(|&&b| b).count();
        eprintln!("\n== VCO 17×17 Jacobian ==");
        eprintln!(
            "  structural nnz : {nnz}/{}  factored L+U : {fnnz}",
            NU * NU
        );
        eprintln!("  worst |natural − dense| rel : {worst_err:.2e} (vs TOL {TOL:.0e})");
        eprintln!("  worst growth factor         : {worst_growth:.2e}");

        let mut s = String::new();
        s.push_str("/// Generated by `vco::tests emit_and_check_vco_lu` — natural-order,\n");
        s.push_str("/// no-pivot LU specialized to the VCO Jacobian's static fill pattern.\n");
        s.push_str("/// DO NOT hand-edit; regenerate if the VCO topology changes.\n");
        s.push_str("#[inline]\n");
        s.push_str(&format!(
            "pub fn vco_lu_solve(mut a: [[f64; {NU}]; {NU}], mut b: [f64; {NU}]) -> [f64; {NU}] {{\n"
        ));
        for col in 0..NU {
            for r in (col + 1)..NU {
                if !fp[r][col] {
                    continue;
                }
                s.push_str(&format!(
                    "    let f{r}_{col} = a[{r}][{col}] / a[{col}][{col}];\n"
                ));
                for c in (col + 1)..NU {
                    if fp[col][c] && fp[r][c] {
                        s.push_str(&format!("    a[{r}][{c}] -= f{r}_{col} * a[{col}][{c}];\n"));
                    }
                }
                s.push_str(&format!("    b[{r}] -= f{r}_{col} * b[{col}];\n"));
            }
        }
        s.push_str(&format!("    let mut x = [0.0f64; {NU}];\n"));
        for i in (0..NU).rev() {
            let mut line = format!("    x[{i}] = (b[{i}]");
            for c in (i + 1)..NU {
                if fp[i][c] {
                    line.push_str(&format!(" - a[{i}][{c}] * x[{c}]"));
                }
            }
            line.push_str(&format!(") / a[{i}][{i}];\n"));
            s.push_str(&line);
        }
        s.push_str("    x\n}\n");
        eprintln!(
            "  generated mul-adds ≈ {} (dense ~{})",
            s.matches("-=").count() + s.matches(" - a[").count(),
            NU * NU * NU / 3
        );
        // Write to a portable temp path so anyone running `--ignored` regen
        // can paste the output into `linalg_static.rs` without a session-
        // specific path hardcoded here.
        let path = std::env::temp_dir().join("vco_lu_gen.rs");
        std::fs::write(&path, &s).unwrap();
        eprintln!("  wrote {}", path.display());
        // Safe iff the no-pivot error is ≪ the Newton TOL it feeds (1e-7), so it
        // can't perturb convergence. Margin ~90× here (growth factor 1.0).
        assert!(
            worst_err < 0.1 * TOL,
            "VCO no-pivot unsafe: rel err {worst_err:.2e} vs 0.1·TOL {:.0e}",
            0.1 * TOL
        );
    }

    #[test]
    fn vco_static_lu_matches_dense() {
        // The generated `vco_lu_solve` must reproduce dense partial-pivot
        // `lu_solve` across pitches, the relaxation cycle (incl. reset edge), and
        // the gmin-homotopy diagonal loading. Permanent guard on the codegen.
        use super::super::linalg_static::vco_lu_solve;
        let sr = 48_000.0f32;
        let mut worst = 0.0f64;
        for &cv in &[2.5f64, 3.5, 4.5, 5.5, 6.5] {
            let mut o = Vco303::raw(sr);
            o.set_cv(cv);
            for i in 0..4000 {
                o.tick();
                if i >= 800 && i % 11 == 0 {
                    let (a, b) = o.capture_system();
                    for extra in [0.0f64, 1e-4, 1e-1] {
                        let mut al = a;
                        for d in 0..NU {
                            al[d][d] += extra;
                        }
                        if let Some(dense) = lu_solve::<NU>(al, b) {
                            let stat = vco_lu_solve(al, b);
                            let rel = (0..NU)
                                .map(|k| (stat[k] - dense[k]).abs() / dense[k].abs().max(1e-9))
                                .fold(0.0, f64::max);
                            worst = worst.max(rel);
                        }
                    }
                }
            }
        }
        // ≪ Newton TOL (1e-7): the static solve cannot perturb convergence.
        assert!(
            worst < 0.1 * TOL,
            "vco static LU diverged: rel err {worst:.2e}"
        );
    }

    #[test]
    #[ignore = "microbench: static vs dense VCO LU throughput"]
    fn bench_vco_static_vs_dense() {
        use super::super::linalg_static::vco_lu_solve;
        use std::hint::black_box;
        use std::time::Instant;
        let sr = 48_000.0f32;
        let mut o = Vco303::raw(sr);
        o.set_cv(4.5);
        for _ in 0..2000 {
            o.tick();
        }
        let (a, b) = o.capture_system();
        const REPS: usize = 5_000_000;
        let t0 = Instant::now();
        let mut acc = 0.0f64;
        for k in 0..REPS {
            let mut bb = b;
            bb[0] += (k & 0xff) as f64 * 1e-12;
            acc += lu_solve::<NU>(black_box(a), black_box(bb)).unwrap()[0];
        }
        let dense = t0.elapsed();
        let t1 = Instant::now();
        let mut acc2 = 0.0f64;
        for k in 0..REPS {
            let mut bb = b;
            bb[0] += (k & 0xff) as f64 * 1e-12;
            acc2 += vco_lu_solve(black_box(a), black_box(bb))[0];
        }
        let stat = t1.elapsed();
        black_box(acc);
        black_box(acc2);
        let dn = dense.as_nanos() as f64 / REPS as f64;
        let sn = stat.as_nanos() as f64 / REPS as f64;
        eprintln!("\n== VCO LU microbench ({REPS} reps) ==");
        eprintln!("  dense  lu_solve<17> : {dn:6.2} ns/solve");
        eprintln!("  static vco_lu       : {sn:6.2} ns/solve");
        eprintln!("  speedup             : {:.2}×", dn / sn);
    }

    /// Natural-order (no-pivot) solve + growth factor, for the no-pivot safety check.
    fn natural_solve(a0: &[[f64; NU]; NU], b0: &[f64; NU]) -> ([f64; NU], f64) {
        let mut a = *a0;
        let mut b = *b0;
        let a0max = a0
            .iter()
            .flat_map(|r| r.iter())
            .fold(0.0f64, |m, &x| m.max(x.abs()));
        let mut growth = a0max;
        for col in 0..NU {
            let d = a[col][col];
            for r in (col + 1)..NU {
                let f = a[r][col] / d;
                if f != 0.0 {
                    for c in col..NU {
                        a[r][c] -= f * a[col][c];
                        growth = growth.max(a[r][c].abs());
                    }
                    b[r] -= f * b[col];
                }
            }
        }
        let mut x = [0.0; NU];
        for i in (0..NU).rev() {
            let mut s = b[i];
            for c in (i + 1)..NU {
                s -= a[i][c] * x[c];
            }
            x[i] = s / a[i][i];
        }
        (x, growth / a0max)
    }

    /// Rising zero-crossings → fundamental of a centered saw.
    fn measure_freq(buf: &[f32], sr: f32) -> f32 {
        let mean = buf.iter().sum::<f32>() / buf.len() as f32;
        let mut zc = 0;
        for w in buf.windows(2) {
            if w[0] - mean < 0.0 && w[1] - mean >= 0.0 {
                zc += 1;
            }
        }
        zc as f32 * sr / buf.len() as f32
    }

    fn render(cv_hz: f32, sr: f32, secs: f32) -> Vec<f32> {
        let mut osc = Vco303::new(sr);
        osc.set_freq(cv_hz);
        let n = (sr * secs) as usize;
        let settle = (sr * 0.10) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..(n + settle) {
            let y = osc.tick();
            if i >= settle {
                out.push(y);
            }
        }
        out
    }

    #[test]
    #[ignore = "diagnostic dump"]
    fn dump_trajectory() {
        let sr = 48_000.0;
        let mut o = Vco303::new(sr);
        o.set_freq(231.0);
        println!("cv={:.3}", super::cv_for_freq(231.0));
        for i in 0..2400 {
            let y = o.tick();
            if i % 30 == 0 {
                println!(
                    "{i:5} cp={:+.3} bf={:+.3} n5={:+.3} n4={:+.3} o_r={:+.3} o_p={:+.3} | or_dc={y:+.4} it={} homo={}",
                    o.node(CP), o.node(BF), o.node(N5), o.node(N4),
                    o.node(O_R), o.node(O_P), o.dbg_iters, o.dbg_homotopy
                );
            }
        }
    }

    fn render_cv(cv: f64, sr: f32, secs: f32) -> Vec<f32> {
        let mut osc = Vco303::new(sr);
        osc.set_cv(cv);
        let n = (sr * secs) as usize;
        let settle = (sr * 0.04) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..(n + settle) {
            let y = osc.tick();
            if i >= settle {
                out.push(y);
            }
        }
        out
    }

    #[test]
    #[ignore = "diagnostic: characterize freq(cv) for MY solver to refit CV_FIT"]
    fn characterize_freq_cv() {
        let sr = 96_000.0;
        // Long render for fine zero-cross resolution.
        for cv in [2.5f64, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5] {
            let f = measure_freq(&render_cv(cv, sr, 0.4), sr);
            println!("cv={cv:.2} freq={f:8.2} log2f={:.4}", (f as f64).log2());
        }
    }

    #[test]
    fn self_oscillates() {
        let sr = 96_000.0;
        let y = render(231.0, sr, 0.1);
        let max = y.iter().cloned().fold(0.0f32, |a, b| a.max(b.abs()));
        let min = y.iter().cloned().fold(0.0f32, |a, b| a.min(b));
        assert!(max > 0.01, "no oscillation: peak {max}");
        assert!((max - min.abs()).is_finite());
        for &s in &y {
            assert!(s.is_finite(), "non-finite sample");
        }
    }

    #[test]
    fn pitch_in_audio_range_and_tracks_cv() {
        let sr = 96_000.0;
        let buf_low = render(231.0, sr, 0.4);
        let f_low = measure_freq(&buf_low, sr);
        let f_hi = measure_freq(&render(462.0, sr, 0.4), sr);
        println!("f_low: {f_low}, f_hi: {f_hi}");
        assert!(
            f_low > 50.0 && f_low < 4_000.0,
            "low pitch out of range: {f_low}"
        );
        // An octave CV step should raise pitch (~tracking; not exact yet).
        assert!(
            f_hi > f_low * 1.4,
            "pitch does not track CV: {f_low} -> {f_hi}"
        );
    }
}
