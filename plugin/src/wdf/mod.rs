//! Pure-Rust port of the BA283 coupled-circuit solver (`~/Git/ba283_wdf/main.cpp`).
//!
//! The real BA283 is a globally-fed-back, DC-coupled 3-transistor amplifier whose mesh is
//! NOT decomposable as a single-root WDF tree, so this solves the full circuit by nodal MNA +
//! Newton-Raphson with companion models (caps & coupled inductors) and Gummel-Poon BJTs — the
//! same method SPICE uses. All math is `f64`; conversion to `f32` happens only at the plugin
//! boundary. No `nih-plug` dependency: this module is validated standalone against C++ golden
//! vectors (see `plugin/tests/test_wdf.rs`).
//!
//! Ported at the **realtime** quality tier: `OS=2, useRB=false (U=16), tol=1e-7, maxIters=30,
//! capTier=FULL, lsTrim=false`, fast transcendentals (`gExactMath=false`).

/// Total node count (`main.cpp:44-67`). With RB off, only `U = 16` of these are unknowns.
pub const NUM_NODES: usize = 21;

/// GND sentinel — node id `-1` reads as 0 V (`main.cpp:68`).
pub const GND: i32 = -1;

/// Node indices, mirroring the C++ `NodeId` enum order exactly. The order defines the `urow`
/// unknown-mapping, so it must not be reordered (`main.cpp:44-66`).
pub mod node {
    pub const N_A: i32 = 0; // +24V rail (known, ramped during soft-start)
    pub const N_INPUT: i32 = 1; // input signal source (known)
    pub const N_C1_MID: i32 = 2; // C1 / ESR junction
    pub const N_R1_MID: i32 = 3; // ESR / R1 junction
    pub const N_TR1_B: i32 = 4;
    pub const N_TR1_BI: i32 = 5; // intrinsic base (after RB)
    pub const N_TR1_E: i32 = 6;
    pub const N_TR1_C: i32 = 7; // == TR2 base (DC-coupled shared node)
    pub const N_TR2_BI: i32 = 8; // TR2 intrinsic base (external base = TR1_C)
    pub const N_J: i32 = 9; // K<->J gain-bridge tap
    pub const N_CGAIN_MID: i32 = 10; // Cgain / ESR junction
    pub const N_TR2_E: i32 = 11; // == TR3 base (compound pair)
    pub const N_TR3_BI: i32 = 12; // TR3 intrinsic base (external base = TR2_E)
    pub const N_TR3_E: i32 = 13;
    pub const N_RV1_W: i32 = 14; // RV1 wiper
    pub const N_F: i32 = 15; // == TR2 collector == TR3 collector / transformer primary input
    pub const N_C4_MID: i32 = 16; // C4 / R5 feedback junction
    pub const N_PA: i32 = 17; // transformer primary internal (after R_pri)
    pub const N_SA: i32 = 18; // transformer secondary internal (before R_sec)
    pub const N_SEC_IN: i32 = 19; // transformer secondary output tap (the listening point)
    pub const N_ZOBEL: i32 = 20; // primary Zobel series-RC mid-node
}

/// Settled-state snapshot for restore-on-reset (`main.cpp` warmup is bounded → snapshot once,
/// restore cheaply). Decimation biquad state is NOT snapshotted (cleared on restore — the
/// transient is sub-millisecond).
#[derive(Clone, Copy, Debug)]
pub struct WdfSnapshot {
    v: [f64; NUM_NODES],
    v_prev: [f64; NUM_NODES],
    caps_i_prev: [f64; MAX_CAPS],
    bjt_state: [(f64, f64, f64, f64); 3], // (qbe_prev, ibe_prev, qbc_prev, ibc_prev)
    i_hist_p: f64,
    i_hist_s: f64,
    v_pprev: f64,
    v_sprev: f64,
    sec_in_dc: f64,
    // JA core state is intentionally NOT snapshotted: the warmup→audio transition (and a knob
    // re-enable) always reseed the core to a clean origin, so any restored value would be
    // immediately clobbered. Keeping it out of the snapshot avoids carrying dead state.
}

/// Coupled-circuit MNA + Newton solver. All scratch is fixed-size (no `Vec` on the process path)
/// to satisfy the zero-alloc audio-thread contract.
///
/// `Clone` is a cheap fixed-size memcpy (no heap), used by the static levelNorm gain probe to run a
/// calibration signal through a scratch copy of the settled model without disturbing the live one.
#[derive(Clone)]
pub struct WdfModel {
    pub v: [f64; NUM_NODES],      // current node voltages
    pub v_prev: [f64; NUM_NODES], // previous-sample node voltages (cap history)
    urow: [i32; NUM_NODES],       // node -> unknown-row index, or -1
    u: usize,                     // number of unknowns

    res: [Res; MAX_RES],
    n_res: usize,
    caps: [Cap; MAX_CAPS],
    n_caps: usize,
    bjts: [Bjt; 3],

    // --- JFET voicing (feature `jfet`): when `jfet` is set, the three `bjts[]` slots hold
    // gate-junction cards (Cgs/Cgd + gate leakage diode) and the square-law channel params live
    // here, one per slot. The Newton device eval routes to the JFET batch kernel instead of the
    // Gummel-Poon one. `jfet=false` ⇒ stock BJT path, bit-exact. See `wdf/jfet/mod.rs`.
    #[cfg(feature = "jfet")]
    jfet: bool,
    #[cfg(feature = "jfet")]
    jfet_params: [JfetExtra; 3],

    // Depletion-charge LUTs (BE/BC per BJT), built once at construction from the fixed junction
    // cards. `Arc` so the audio-thread `measure_gain` clone is a refcount bump, not a deep copy
    // (preserving the no-heap-on-clone invariant). Used only when `dep_lut` is set (lossy tiers).
    dep_luts: Arc<DepLutSet>,

    r_kj_idx: usize,    // R_KJ resistor index (gain knob), for set_rgain
    r_upper_idx: usize, // RV1 upper-leg index, for set_bias
    r_lower_idx: usize, // RV1 lower-leg index

    // --- component aging (deterministic per-unit drift) ---
    res_base_g: [f64; MAX_RES],   // pristine 1/ohms captured at build (incl. the live bias/rgain legs)
    cap_base_far: [f64; MAX_CAPS], // pristine farads captured at build
    age_pattern: AgePattern,      // per-unit signed deviations (seed-derived, age-independent)
    age: f64,                     // current knob value [0,1]; 0 => no drift
    age_seed: u64,                // current unit seed
    res_material: [ResMat; MAX_RES],   // per-leg material → tolerance band
    cap_material: [CapMat; MAX_CAPS],  // per-cap dielectric → aging behaviour
    tol_grade: TolGrade,          // build grade; scales every drift band (Ideal => no drift)

    // transformer coupled-inductor pristine values (Lp, Ls, K) — for the aging recompute of `gl`
    lp_base: f64,
    ls_base: f64,
    k_coupl_base: f64,

    // coupled inductor companion (transformer, trapezoidal): g = (dt/2) * inv([Lp M; M Ls])
    gl: [[f64; 2]; 2],
    i_hist_p: f64, // primary winding current i[n-1]
    i_hist_s: f64, // secondary winding current i[n-1]
    v_pprev: f64,  // winding voltages at n-1
    v_sprev: f64,

    // --- nonlinear JA transformer core (ON/OFF switch + HYST/SAT dials; feature `trafo-core`) ---
    #[cfg(feature = "trafo-core")]
    trafo_hyst: f64, // last HYSTERESIS dial (cached so age re-apply re-voices from the same point)
    #[cfg(feature = "trafo-core")]
    trafo_sat: f64, // last SATURATION dial
    #[cfg(feature = "trafo-core")]
    ja_active: bool, // ON/OFF gate; false => linear, bit-exact
    #[cfg(feature = "trafo-core")]
    ja: JaCore,
    #[cfg(feature = "trafo-core")]
    ja_m: f64, // magnetization M[n-1]
    #[cfg(feature = "trafo-core")]
    ja_h_prev: f64, // H[n-1]
    #[cfg(feature = "trafo-core")]
    ja_phi_prev: f64, // flux phi[n-1]
    #[cfg(feature = "trafo-core")]
    ja_vm_prev: f64, // magnetizing branch voltage v_m[n-1]
    #[cfg(feature = "trafo-core")]
    ja_delta: f64, // frozen field-rate direction sign (+1/-1)
    #[cfg(feature = "trafo-core")]
    ja_i_dev: f64, // lagged magnetizing-deviation current trafo*(i_JA - i_lin), injected next sample

    dt: f64,
    newton_tol: f64,
    max_iters: usize,
    cap_tier: CapTier,

    // reusable solver scratch (only first `u` entries used)
    hist: [f64; NUM_NODES],
    f: [f64; NUM_NODES],
    delta: [f64; NUM_NODES],
    rn: [f64; NUM_NODES],
    j: [f64; NUM_NODES * NUM_NODES],
    g0: [f64; NUM_NODES * NUM_NODES], // constant linear conductance matrix

    // --- round-2 speed levers (off at the realtime tier) ---
    predict: bool,
    solver: Solver,
    freeze_caps: bool,
    ls_trim: bool,
    fma: bool, // T3: FMA contraction in device kernels (lossy fast/ultra); false at realtime
    dep_lut: bool, // use the depletion-charge LUT (lossy fast/ultra); false at realtime (pow path)
    fixed_step: bool, // exactly max_iters, no residual_norm gate / line search; only `measure_gain`
    os: usize, // internal oversampling factor (host_rate * os = solve rate); 2 normally, 1 ultra-fast
    decimate: bool, // apply output decimation biquads (needless at os==1)

    /// In-solve ADAA1 spike toggle (`adaa-insolve` feature, off by default — never shipped). When
    /// set, the BJT transport exponentials are evaluated through their first-order antiderivative-
    /// antialiased form inside the Newton solve. Runtime flag so the harness can A/B baseline vs ADAA
    /// on one warmed model. See `compute_bjt_preexp_batch_adaa` / `adaa1_exp`.
    #[cfg(feature = "adaa-insolve")]
    adaa: bool,

    /// Path-1 dominant-variable ADAA toggle (`adaa-port` feature, off by default — never shipped).
    /// When set, ONLY the dominant forward base-emitter transport arm is antialiased per BJT; the
    /// reverse (BC) arm and all cross terms stay instantaneous (JAES §6.3 dominant-variable
    /// reduction). Shares the `adaa1_exp` / `compute_bjt_preexp_batch_adaa` substrate with `adaa`.
    #[cfg(feature = "adaa-port")]
    adaa_port: bool,

    // Newton predictor: cross-sample voltage extrapolation (audio pass only).
    v_pred_prev: [f64; NUM_NODES], // sol(n-2) for the unknown nodes
    predict_primed: bool,          // false until the first extrapolation seed exists
    in_audio_pass: bool,           // off during DC warmup/rail-ramp/settle

    // Freeze-caps: cached iter-0 junction-cap companion per BJT: (cbe, cbc, off_be, off_bc).
    frozen_cap: [(f64, f64, f64, f64); 3],
    frozen_caps_valid: bool,

    // Trailing-state reuse (lossless): the converged junction charges (qbe, qbc) per BJT, stored by
    // the Newton iter that hits the convergence break. The per-sample trapezoidal trailing update
    // then reuses them instead of re-running diode_branch + depletion_charge_pre at the same v.
    // Valid only when the solve converged via the tol break AND freeze_caps is off — the frozen
    // companion (Q = off + C*v) is an approximation, not the true charge, so fast/ultra recompute.
    cap_q_cache: [(f64, f64); 3],
    cap_cache_valid: bool,

    // Schur static-block partition (built once) + per-sample scratch.
    n_idx: [usize; NUM_NODES], // global unknown-row indices in the N (nonlinear-coupled) set
    l_idx: [usize; NUM_NODES], // global unknown-row indices in the L (linear-only) set
    n_sz: usize,
    l_sz: usize,
    schur_ready: bool,
    s_ll_fac: [f64; NUM_NODES * NUM_NODES], // LU factors of A_LL (row-permuted, in-place)
    s_ll_piv: [usize; NUM_NODES],
    s_w: [f64; NUM_NODES * NUM_NODES],      // W = A_LL^{-1} A_LN
    s_sconst: [f64; NUM_NODES * NUM_NODES], // S_const = A_NN_lin - A_NL W
    s_anl: [f64; NUM_NODES * NUM_NODES],
    s_aln: [f64; NUM_NODES * NUM_NODES],
    s_annlin: [f64; NUM_NODES * NUM_NODES],
    s_s: [f64; NUM_NODES * NUM_NODES], // per-iter Schur complement

    // oversampling / decimation driver (set by `new`; unused when `build` is called directly)
    dec1: Biquad, // legacy decimator section 1 (4th-order Butterworth); used only when legacy_resample
    dec2: Biquad, // legacy decimator section 2
    /// Steep linear-phase FIR resampler (windowed-sinc polyphase up + decimate). `Some` on the
    /// shipped path (`!legacy_resample` and `os > 1`); replaces the lerp upsample + Butterworth
    /// decimator with a flat-to-20kHz response at the cost of a reported group-delay latency
    /// (`fir_latency_host`). `None` for legacy (golden parity) and os==1 (nothing to resample).
    fir: Option<FirResampler>,
    /// Mirrors `cfg.legacy_resample`. Retained so the model can report a TIER-INVARIANT FIR latency
    /// (`fir_reported_latency_host`): non-legacy tiers all report `FIR_LATENCY_HOST` regardless of
    /// `os` (the os==1 tiers have `fir == None` but still report the constant so the host PDC never
    /// changes on a live tier switch), while the legacy path reports 0 (golden-parity, 0-PDC).
    legacy_resample: bool,
    sec_in_dc: f64, // output DC offset captured at warmup; subtracted from the tap
    prev_in: f64,   // previous host input sample (legacy linear-interpolation upsampling state)
    last_snapshot: Option<WdfSnapshot>,

    // Newton-iteration instrumentation (write-only side state; never read on the audio path, so it
    // cannot perturb the solve). Mirrors the C++ `samples`/`totalIters`/`maxItersUsed` counters
    // (`main.cpp:1314-1320`). Each `process()` adds its total iters (iter-0 solve + any gmin
    // homotopy passes) and bumps the per-call max. Reset via `reset_iter_stats` before an audio pass.
    iter_samples: u64,
    iter_total: u64,
    iter_max: usize,

    // --- sub-rate Newton spike (behind `subrate-spike` feature; zero overhead in default build) ---
    #[cfg(feature = "subrate-spike")]
    subrate: usize,        // full Newton every K-th internal audio sample (1 = off)
    #[cfg(feature = "subrate-spike")]
    subrate_hold: bool,    // naive-hold baseline: freeze v on skipped samples (no linear update)
    #[cfg(feature = "subrate-spike")]
    subrate_phase: usize,  // internal audio samples since last full solve (0 => full solve due)
    #[cfg(feature = "subrate-spike")]
    subrate_valid: bool,   // a frozen model has been captured and is usable
    #[cfg(feature = "subrate-spike")]
    capture_j: bool,       // gate: snapshot the converged J in `newton_iterate` (full-solve sample)
    #[cfg(feature = "subrate-spike")]
    j_captured: bool,      // set by `newton_iterate` when it actually snapshot a converged J
    #[cfg(feature = "subrate-spike")]
    j_frozen_raw: [f64; NUM_NODES * NUM_NODES], // converged J at the last full solve (= g0 + J_dev)
    #[cfg(feature = "subrate-spike")]
    j_dev: [f64; NUM_NODES * NUM_NODES],        // j_frozen_raw - g0 (frozen device conductances)
    #[cfg(feature = "subrate-spike")]
    lu_frozen: [f64; NUM_NODES * NUM_NODES],    // LU factors of j_frozen_raw (stride u)
    #[cfg(feature = "subrate-spike")]
    piv_frozen: [usize; NUM_NODES],
    #[cfg(feature = "subrate-spike")]
    i0_const: [f64; NUM_NODES],                 // frozen device Norton constant (reduced rows)
}

/// RV1 (4k7 preset) split. `bias` in [0,1] is remapped onto the stable pot range then split into
/// the two legs (`main.cpp:307-312`). Returns `(r_upper, r_lower)`.
fn rv1_split(bias: f64) -> (f64, f64) {
    const RV1_WIPER_MIN: f64 = 0.3;
    let wiper = RV1_WIPER_MIN + (1.0 - RV1_WIPER_MIN) * bias;
    let r_upper = (4700.0 * (1.0 - wiper)).max(1.0);
    let r_lower = (4700.0 * wiper).max(1.0);
    (r_upper, r_lower)
}


mod components;
mod math;
mod bjt;
#[cfg(feature = "jfet")]
mod jfet;
mod config;
mod aging;
mod accessors;
mod build;
mod params;
mod solvers;
mod schur;
mod newton;
mod frozen;
mod process;
mod resample;
mod snapshot;

// Internal re-imports: bring submodule `pub(in crate::wdf)` helpers (e.g. `exp_bl`,
// `BjtOutSoA`, `MAX_RES`) into this module's scope so sibling submodules resolve them
// via `use super::*`. Only the modules that export such helpers are needed here.
use bjt::*;
#[cfg(feature = "jfet")]
use jfet::*;
use config::*;
use math::*;
use resample::FirResampler;
use std::sync::Arc;

// Public API surface — explicit re-exports reproduce the pre-split `crate::wdf::*`
// paths exactly (consumed by `dsp.rs` and the integration-test/bench/bin crates).
pub use aging::{reroll_seed, AgePattern};
#[cfg(feature = "trafo-core")]
pub use aging::JaCore;
pub use bjt::{compute_bjt, BjtOut};
#[cfg(feature = "bench-internals")]
pub use bjt::depletion_charge_pre_batch;
pub use components::{
    bc184c, dep_sat_consts, q2n3055, Bjt, BjtParams, Biquad, Cap, DepLut, DepLutSet, Res,
};
pub use config::{CapTier, Oversampling, Precision, QualityConfig, Solver, OS};
pub use resample::FIR_LATENCY_HOST;
pub use math::{
    depletion_charge, depletion_charge_pre, diode_branch, exp_, fast_exp, fast_log,
    fast_pow_pos, pow_, ultra_exp, EXACT,
};
