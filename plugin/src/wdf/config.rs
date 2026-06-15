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
    /// iteration-capping leaves running. Lossy (no convergence guarantee on transients); set by the
    /// Med/Low Precision settings only — realtime/HIGH keep `false` and stay bit-identical.
    pub fixed_step: bool,
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
            ..Self::realtime()
        }
    }
    /// Ultra-fast tier: `fast()` + the cheap low-order transport exp (`cheap_exp`) + its dominant
    /// speed lever, internal oversampling dropped to os=1 (decimation biquads bypassed).
    ///
    /// os=1 is the ~1.8× speed win, but it limit-cycles into loud output peaks ONLY when combined with
    /// the Med/Low fixed-step solver (2 iters, no convergence gate) — raw peak ~114 vs ~62 at High. At
    /// High (full convergence) os=1 is peak-safe (raw ≈ os=2) and validated −30 dB vs SPICE. So the os
    /// floor is CONDITIONAL: [`QualityConfig::solve_os`] forces os≥2 (re-enabling the antialiasing
    /// decimator via [`solve_decimate`](Self::solve_decimate)) ONLY under `fixed_step`. This tier
    /// therefore runs os=1 at High precision (fast) and is automatically floored to os=2 at Med/Low
    /// (safe). Previously os was floored at 2 unconditionally, which also removed the speed lever from
    /// the safe High path.
    pub fn ultra_fast() -> Self {
        Self {
            cheap_exp: true, // T3: low-order transport exp (~2%); ultra-fast tier accepts the op-point bias
            os: 1,           // dominant speed lever; floored to 2 under fixed_step (see solve_os)
            decimate: false, // nothing to decimate at os=1; re-enabled if solve_os floors os up to 2
            ..Self::fast()
        }
    }

    /// `ultra_fast()` with the BJT junction charge model dropped (`cap_tier = Off`): the depletion +
    /// diffusion charge batch and its trapezoidal trailing update (~19 % of the per-sample cost) are
    /// removed entirely, leaving transport-only devices. Measured 2026-06-09 (`ba283_standalone
    /// --backend wdf`, sweep_m6): 1.30× over ultra-fast (≈4.2× over realtime). The caps shape only
    /// the very top octave, so dropping them perturbs the output by just
    /// −55 dB vs ultra-fast — far below the −23 dB WDF-vs-ACME model floor — i.e. vs SPICE it stays
    /// within a hair of ultra-fast's −30 to −36 dB. Distinct from `with_precision(Low)`, which also
    /// sets `cap_tier = Off` but additionally forces the fixed-step solver (max_iters = 2); this tier
    /// keeps the full convergence gate, so the only change vs ultra-fast is the charge model.
    pub fn warp() -> Self {
        Self {
            cap_tier: CapTier::Off,
            ..Self::ultra_fast()
        }
    }

    /// Layer a lossy [`Precision`] setting onto this tier config. `High` is the identity (preserves
    /// the bit-exact realtime/golden contract); `Medium`/`Low` enable the fixed-step solver (exactly 2
    /// Newton iters, no `residual_norm` gate / line search) and drop the BJT charge model — these are
    /// the structural levers the `eval/wdf_ultrafast` sweep never tested (looser tol / capped iters
    /// alone buy ~nothing because the per-sample `residual_norm` keeps running). Gated by the harness
    /// at ESR loss ≤ −50 dB vs Realtime+High. (f32 device kernels and sub-rate solving land in later
    /// phases; this is the in-house, low-risk first cut.)
    ///
    /// `predict` is forced OFF under fixed-step. The Newton predictor extrapolates the operating
    /// point across samples (`2·sol(n-1) − sol(n-2)`); with only 2 fixed iters and no `residual_norm`
    /// gate or `ls_trim` line search to reject a bad warm-start, that extrapolation drifts, the solve
    /// can't pull it back, and the next sample's predict amplifies it — a runaway to ~1e8 ("loud
    /// garbage then dead channel") on every non-realtime tier at Med/Low. Realtime+Med/Low never hit
    /// this only because the realtime base already has `predict=false`. Confirmed by lever isolation
    /// (`tests/test_precision_tiers.rs::isolate_lever_fast_medium`): predict=false bounds the output
    /// to ~1.5e2 (≈ the full-convergence level), while Dense/freeze_caps=false/fma=false do not.
    pub fn with_precision(self, p: Precision) -> Self {
        match p {
            Precision::High => self,
            Precision::Medium => Self {
                fixed_step: true,
                predict: false,
                max_iters: 2,
                cap_tier: CapTier::Depletion,
                ..self
            },
            Precision::Low => Self {
                fixed_step: true,
                predict: false,
                max_iters: 2,
                cap_tier: CapTier::Off,
                ..self
            },
        }
    }

    /// Effective internal oversampling, the single source of truth used by both `build_with` and
    /// `new_with`. os=1 is allowed for the full-convergence solver (the ultra-fast/warp speed lever,
    /// peak-safe at High), but the fixed-step solver (Med/Low precision) limit-cycles into loud peaks
    /// at os=1, so it is floored to 2. No *built or rebuilt* model — including one from a
    /// hand-assembled `fixed_step` config — can solve at os=1. (The lone exception is the near-linear
    /// `measure_gain` probe, which mutates an already-built model directly; see its NOTE.)
    pub fn solve_os(&self) -> usize {
        if self.fixed_step {
            self.os.max(2)
        } else {
            self.os.max(1)
        }
    }

    /// Effective decimation. When [`solve_os`](Self::solve_os) floors os up to 2 under `fixed_step`
    /// (the tier had set os=1 / decimate=false), the antialiasing decimator MUST run on the now-
    /// oversampled stream, even though the tier requested no decimation. Otherwise honour the tier.
    pub fn solve_decimate(&self) -> bool {
        self.decimate || (self.fixed_step && self.os < 2)
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
pub(crate) const MAX_RES: usize = 24;
pub(crate) const MAX_CAPS: usize = 8;

// --- component aging drift bands (max signed deviation at age=1.0) ---
// Resistors drift up/down a few percent (carbon/metal-film aging + tolerance spread).
pub(crate) const R_DRIFT_MAX: f64 = 0.05; // ±5%
// Film caps are stable; small symmetric spread.
pub(crate) const C_FILM_DRIFT: f64 = 0.05; // ±5%
// Electrolytics lose capacitance with age (dry-out): directional, mostly downward.
pub(crate) const C_ELEC_DROP_MAX: f64 = 0.20; // up to -20%
pub(crate) const C_ELEC_DROP_MIN: f64 = 0.02; // up to +2%
// Farad threshold separating electrolytic (drift directional) from film (drift symmetric).
pub(crate) const C_ELEC_THRESH_F: f64 = 1.0e-6;
