// ---------------------------------------------------------------------------
// WdfModel — the circuit: topology + integrator state, solves one sample via Newton.
// Ported from `class BA283` (`main.cpp:255-794`) at the realtime tier, RB off.
// ---------------------------------------------------------------------------

/// BJT charge-storage model detail (`main.cpp:72`). Realtime tier uses `Full`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CapTier {
    #[default]
    Full,
    Depletion,
    Off,
}

// Realtime quality tier (`qualityPreset("realtime")`, main.cpp:84): OS=2, RB off, tol 1e-7,
// maxIters 30, caps full, lsTrim off. RB off pins the 3 intrinsic-base nodes -> U = 16.
const RT_NEWTON_TOL: f64 = 1e-7;
const RT_MAX_ITERS: usize = 30;

/// Linear-solve path (`main.cpp:76`, `SolverMode`). `Dense` = full Gaussian elimination;
/// `Schur` = static-block solve (factor the constant linear block once per sample, solve only
/// the tiny nonlinear Schur complement per Newton iter). Schur falls back to dense when
/// `gmin != 0` (homotopy perturbs the linear block) or when the complement is singular.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Solver {
    #[default]
    Dense,
    Schur,
}

/// Quality/speed levers (`main.cpp:77` `QualityConfig` + the per-model lever fields).
/// `realtime()` = every lever off (bit-identical to the reference); `fast()` = predict + schur
/// + freeze-caps + ls-trim all on (`qualityPreset("fast")`, ~0.195x realtime, near-transparent).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QualityConfig {
    pub newton_tol: f64,
    pub max_iters: usize,
    pub cap_tier: CapTier,
    /// 1st-order cross-sample Newton warm start (`--predict`). Lossless.
    pub predict: bool,
    /// Linear-solve path (`--solver schur`). Lossless within tol.
    pub solver: Solver,
    /// Freeze the junction-cap companion at Newton iter 0 (`--freeze-caps`). Lossy (FR ±0.1 dB).
    pub freeze_caps: bool,
    /// Deferred-acceptance line search (`--ls-trim`). Lossless.
    pub ls_trim: bool,
    /// Cheap low-order (`ultra_exp`, ~1e-4 rel) transport exponential instead of the
    /// near-exact `fast_exp` (`--cheap-exp`). Lossy: perturbs the BJT operating point
    /// slightly (small DC/gain bias). Ultra-fast tier only.
    pub cheap_exp: bool,
    /// Internal oversampling factor (host_rate × os = solve rate). 2 at realtime/fast; the
    /// ultra-fast tier drops to 1 (`--os 1`) — the dominant speed lever (halves the solve rate),
    /// lossy via reduced nonlinear-harmonic antialiasing. Gated vs SPICE, not vs realtime.
    pub os: usize,
    /// Apply the output decimation biquads. At `os > 1` these are the antialiasing decimator and
    /// MUST stay on; at `os == 1` there is nothing to decimate, so they only add phase/group delay
    /// — bypassing (`--no-decimate`) can recover some of the os=1 fidelity cost. Default on.
    pub decimate: bool,
    /// FMA (fused multiply-add) contraction in the batched device kernels (`compute_bjt_preexp_batch`,
    /// `exp_bl`/`log_bl`, depletion charge). Single-rounding `vfmadd` instead of `a*b+c`; ~1 ULP/op
    /// numeric change → measured +2.3% (fast) / +3.8% (ultra), ~half the fp-contract=fast ceiling.
    /// Lossy: off at realtime (bit-identical), on for fast/ultra (gated by `compare.py` −40 dB ESR).
    pub fma: bool,
    /// Fixed-step Newton: run exactly `max_iters` iterations and drop the convergence machinery —
    /// no `ls_trim` line search and no per-sample `residual_norm` gate / gmin-homotopy (`process`).
    /// Removes the dominant fixed per-sample cost (one full device re-stamp every sample) that
    /// iteration-capping leaves running. Lossy (no convergence guarantee on transients).
    ///
    /// No tier preset and no `Precision` setting enables this (precision is device-model-only). The
    /// sole user is [`WdfModel::measure_gain`], which sets it on a *cloned, already-built* model for a
    /// cheap deterministic 0.05 V small-signal gain probe on the audio thread — it bypasses
    /// [`solve_os`](Self::solve_os), runs near-linear, and never enters the program-drive regime that
    /// motivated the old os floor. Realtime/HIGH keep `false` and stay bit-identical.
    pub fixed_step: bool,
    /// Depletion-charge LUT: replace the per-junction `pow` evaluation in the depletion-charge
    /// kernel with a precomputed 1-D table + linear interp (the `pow` arm `v < FC*vj` only; the
    /// linear arm above `FC*vj` is unchanged). Lossy — added error vs realtime is well below the
    /// −40 dB budget (cap rms ≈ −74 dB at N=512), but not bit-exact. Measured ~7.5× on the isolated
    /// device-eval kernel. On for fast/ultra-fast (and `with_precision(Medium)`); off at realtime
    /// (bit-identical / golden) and a no-op on warp / `Precision::Low` (`cap_tier = Off`, no charge).
    pub dep_lut: bool,
    /// Sub-rate Newton spike lever — **NO-GO, behind `subrate-spike` feature only**.
    /// See the full verdict in the `#[cfg(feature = "subrate-spike")]`-gated field below.
    /// Not present in the default build; compile with `--features subrate-spike` to access.
    #[cfg(feature = "subrate-spike")]
    /// Run the full nonlinear Newton solve only every `subrate`-th internal sample; on the
    /// in-between samples freeze the converged Jacobian (the BJTs' Norton linearization) and its LU
    /// factorization and do one linear back-substitution — skipping all `exp4`/depletion/Gummel-Poon
    /// assembly + the matrix factor. `1` = off (every sample, fully inert / bit-exact). Lossy
    /// (operating point lags ≤ subrate-1 samples); gated vs Realtime+High and vs SPICE, never vs
    /// C++ parity (the golden has no such path).
    ///
    /// VERDICT 2026-06-09: **NO-GO**, kept as a hidden A/B lever only. The `eval/wdf_ultrafast` sweep
    /// confirmed avgIters drops as designed (fast 1.75 → K2 1.08 / K3 0.84 / K4 0.55, i.e. the freeze
    /// really does skip device Newton iters) but **two independent gates fail**: (1) speed — K=2 is
    /// *slower* than fast (0.88×), best K=4 only 1.06× (target ≥1.3×), because the frozen path swaps
    /// the fast tier's cheap Schur small-block factor for a full DENSE O(u³) LU factor on every
    /// capture + dense O(u²) back-sub on every skip, which costs ~what the skipped device evals save;
    /// (2) fidelity — added-ESR vs Realtime+High is only −37.9 dB at K=2 (budget ≤ −50 dB), and the
    /// frozen op-point lag error is intrinsic, not an implementation artifact. The profiler's
    /// long-standing read held: iteration count was never the bottleneck — the fixed per-sample
    /// device cost is, and sub-rate just trades it for its own per-sample factor/solve. Do not retry
    /// the dense-frozen form; a Schur-reuse variant could recover (1) but cannot fix (2).
    #[cfg(feature = "subrate-spike")]
    pub subrate: usize,
    /// Sub-rate naive-hold baseline (spike measurement only): on a skipped sample hold `v` frozen
    /// instead of running the frozen linear solve. Quantifies what the linear update buys. Ignored
    /// when `subrate <= 1`.
    #[cfg(feature = "subrate-spike")]
    pub subrate_hold: bool,
}

/// Lossy precision setting, orthogonal to [`WdfTier`](crate). A modifier layered on a tier's
/// `QualityConfig` via [`QualityConfig::with_precision`]: `High` is the identity (so Realtime+High
/// stays bit-identical / golden), `Medium`/`Low` trade output fidelity for speed within the harness
/// budget (ESR loss vs Realtime+High ≤ −50 dB; ACME absolute floor is ~−23 dB, model-bound).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    High,
    Medium,
    Low,
}

/// Internal-oversampling override, orthogonal to [`WdfTier`](crate) and [`Precision`]. The new
/// antialiasing-rate axis. `Auto` defers to the tier's own `os` (so UltraFast/Warp keep their os=1
/// speed lever and Realtime/Fast keep os=2); `X1`/`X2` force the rate. Only 1×/2× ship now — the
/// decimator is a 2×-tuned Butterworth (aliases at 4:1); `X4` lands once a 4:1 decimator exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Oversampling {
    Auto,
    X1,
    X2,
}

impl QualityConfig {
    pub fn realtime() -> Self {
        Self {
            newton_tol: RT_NEWTON_TOL,
            max_iters: RT_MAX_ITERS,
            cap_tier: CapTier::Full,
            predict: false,
            solver: Solver::Dense,
            freeze_caps: false,
            ls_trim: false,
            cheap_exp: false,
            os: OS,
            decimate: true,
            fma: false,
            fixed_step: false,
            dep_lut: false, // realtime stays bit-exact (golden) — pow path, no table
            #[cfg(feature = "subrate-spike")]
            subrate: 1,
            #[cfg(feature = "subrate-spike")]
            subrate_hold: false,
        }
    }
    pub fn fast() -> Self {
        Self {
            predict: true,
            solver: Solver::Schur,
            freeze_caps: true,
            ls_trim: true,
            fma: true, // T3: FMA contraction in device kernels (lossy, +2.3% fast); realtime stays false
            dep_lut: true, // depletion-charge LUT (lossy, ~7.5× on the device-eval kernel); inherited by ultra/warp
            ..Self::realtime()
        }
    }
    /// Ultra-fast tier: `fast()` + the cheap low-order transport exp (`cheap_exp`) + its dominant
    /// speed lever, internal oversampling dropped to os=1 (decimation biquads bypassed).
    ///
    /// os=1 is the ~1.8× speed win and is peak-safe under the full-convergence solver this tier runs
    /// (raw peak ≈ os=2, validated −30 dB vs SPICE). Precision is now device-model-only and never
    /// enables `fixed_step`, so there is no os floor and Med/Low no longer slow this tier down — they
    /// only step the BJT model on top of os=1.
    pub fn ultra_fast() -> Self {
        Self {
            cheap_exp: true, // T3: low-order transport exp (~2%); ultra-fast tier accepts the op-point bias
            os: 1,           // dominant speed lever (halves the solve rate)
            decimate: false, // nothing to decimate at os=1
            ..Self::fast()
        }
    }

    /// `ultra_fast()` with the BJT junction charge model dropped (`cap_tier = Off`): the depletion +
    /// diffusion charge batch and its trapezoidal trailing update (~19 % of the per-sample cost) are
    /// removed entirely, leaving transport-only devices. Measured 2026-06-09 (`ba283_standalone
    /// --backend wdf`, sweep_m6): 1.30× over ultra-fast (≈4.2× over realtime). The caps shape only
    /// the very top octave, so dropping them perturbs the output by just
    /// −55 dB vs ultra-fast — far below the −23 dB WDF-vs-ACME model floor — i.e. vs SPICE it stays
    /// within a hair of ultra-fast's −30 to −36 dB. Same `cap_tier = Off` as `with_precision(Low)`,
    /// but reached via the tier rather than the precision axis: warp keeps full convergence (no
    /// `fixed_step`), so the only change vs ultra-fast is the charge model.
    pub fn warp() -> Self {
        Self {
            cap_tier: CapTier::Off,
            ..Self::ultra_fast()
        }
    }

    /// Layer a lossy [`Precision`] setting onto this tier config. Precision is the **device-model
    /// fidelity** axis (orthogonal to the tier's iteration strategy and the oversampling rate): it
    /// steps `cap_tier` and `cheap_exp` *monotonically downward* from the tier baseline and touches
    /// nothing else. `High` is the identity (preserves the bit-exact realtime/golden contract);
    /// `Medium`/`Low` trade BJT-model fidelity for speed within the harness budget (ESR loss vs
    /// Realtime+High ≤ −50 dB; ACME absolute floor ~−23 dB, model-bound).
    ///
    /// - **Medium** → drop the junction charge model to depletion-only (`cap_tier = Depletion`,
    ///   never *up*grading a tier already at `Off`) **and** the cheap low-order transport exp
    ///   (`cheap_exp = true`). Depletion-vs-Full is ~0 CPU (fidelity-only, measured 6.23→6.21 s on
    ///   Fast), so `cheap_exp` (~−3%) is what makes Medium a real monotone step rather than a placebo.
    /// - **Low** → drop the charge model entirely (`cap_tier = Off`, the dominant lever, −26% on Fast)
    ///   plus `cheap_exp = true`.
    ///
    /// Precision no longer touches `fixed_step` / `predict` / `max_iters` / `os` — those are the
    /// tier's iteration strategy and the oversampling axis. (Previously Med/Low forced the fixed-step
    /// solver, which on os=1 tiers floored os 1→2 via [`solve_os`], making "lower precision" *slower*;
    /// that perverse coupling is gone.) Gated by the harness (`eval/`).
    pub fn with_precision(self, p: Precision) -> Self {
        match p {
            Precision::High => self,
            // Monotone downgrade: never upgrade a tier already at a lower fidelity (e.g. warp is
            // already cap_tier=Off, so Medium leaves it Off rather than raising it to Depletion).
            Precision::Medium => Self {
                cap_tier: if self.cap_tier == CapTier::Off {
                    CapTier::Off
                } else {
                    CapTier::Depletion
                },
                cheap_exp: true,
                // Medium is explicitly lossy: enable the depletion-charge LUT whenever caps stay on.
                dep_lut: self.cap_tier != CapTier::Off,
                ..self
            },
            Precision::Low => Self {
                cap_tier: CapTier::Off,
                cheap_exp: true,
                ..self
            },
        }
    }

    /// Layer an [`Oversampling`] override onto this tier config — the antialiasing-rate axis. `Auto`
    /// is the identity (defers to the tier's `os`/`decimate`, preserving current behaviour bit-for-
    /// bit); `X1` forces os=1 with the decimator bypassed (nothing to decimate); `X2` forces os=2
    /// with the antialiasing decimator on. Applied after [`with_precision`](Self::with_precision) in
    /// the config chain so it is the final word on the rate.
    pub fn with_oversampling(self, os: Oversampling) -> Self {
        match os {
            Oversampling::Auto => self,
            Oversampling::X1 => Self { os: 1, decimate: false, ..self },
            Oversampling::X2 => Self { os: 2, decimate: true, ..self },
        }
    }

    /// Effective internal oversampling, the single source of truth used by both `build_with` and
    /// `new_with`. The oversampling rate is now a pure config value: os=1 is the ultra-fast/warp
    /// speed lever (peak-safe at full convergence), os=2 the realtime/fast antialiasing rate. No
    /// config built through `build_with` sets `fixed_step` (precision is device-model-only), so there
    /// is no longer an os floor — the lone `fixed_step` user, the near-linear `measure_gain` probe,
    /// mutates an already-built model directly and bypasses this (see its NOTE).
    pub fn solve_os(&self) -> usize {
        self.os
    }

    /// Effective decimation — honour the tier. (Historically this re-enabled the decimator when an
    /// os floor kicked in under `fixed_step`; with the floor gone it is just the tier's setting.)
    pub fn solve_decimate(&self) -> bool {
        self.decimate
    }
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self::realtime()
    }
}

/// Internal oversampling factor (realtime tier). Host rate × OS = internal solve rate.
pub const OS: usize = 2;

// Worst-case fixed-array bounds (RB off uses 17 resistors / 7 caps; sized for RB-on headroom).
pub(in crate::wdf) const MAX_RES: usize = 24;
pub(in crate::wdf) const MAX_CAPS: usize = 8;

// --- component aging drift bands (max signed deviation at age=1.0) ---
// Resistors drift up/down a few percent (carbon/metal-film aging + tolerance spread).
pub(in crate::wdf) const R_DRIFT_MAX: f64 = 0.05; // ±5%
// Film caps are stable; small symmetric spread.
pub(in crate::wdf) const C_FILM_DRIFT: f64 = 0.05; // ±5%
// Electrolytics lose capacitance with age (dry-out): directional, mostly downward.
pub(in crate::wdf) const C_ELEC_DROP_MAX: f64 = 0.20; // up to -20%
pub(in crate::wdf) const C_ELEC_DROP_MIN: f64 = 0.02; // up to +2%
// Farad threshold separating electrolytic (drift directional) from film (drift symmetric).
pub(in crate::wdf) const C_ELEC_THRESH_F: f64 = 1.0e-6;
