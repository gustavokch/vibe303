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

/// BJT device card (`main.cpp:89-97`). `alpha_f`/`alpha_r` are retained but unused by the
/// Gummel-Poon path. `bf`/`br`/`vaf`/`var`/`ikf`/`ikr` <= 0 mean the corresponding effect is off.
#[derive(Clone, Copy, Debug, Default)]
pub struct BjtParams {
    pub is: f64,
    pub vt: f64,
    pub alpha_f: f64,
    pub alpha_r: f64,
    pub vcrit: f64,
    pub cje: f64,
    pub vje: f64,
    pub mje: f64,
    pub cjc: f64,
    pub vjc: f64,
    pub mjc: f64,
    pub tf: f64,
    pub tr: f64,
    pub bf: f64,
    pub br: f64,
    pub vaf: f64,
    pub var: f64,
    pub ikf: f64,
    pub ikr: f64,
}

// vcrit = Vt*ln(Vt/(sqrt(2)*Is)). The C++ device cards use the truncated literal `1.41421356`
// (NOT std::sqrt(2)); we match it verbatim so vcrit — and every downstream BJT value — is
// bit-identical to the golden vectors. This is a one-time build constant, so std `ln` (not the
// fast-math path) is correct here, matching the C++ `std::log`.
const SQRT2_TRUNC: f64 = 1.41421356;

/// BC184C small-signal card (TR1, TR2) — `main.cpp:296-298`.
pub fn bc184c() -> BjtParams {
    let is = 1.04e-14;
    BjtParams {
        is,
        vt: 0.02585,
        alpha_f: 0.99834,
        alpha_r: 0.0909,
        vcrit: 0.02585 * (0.02585 / (SQRT2_TRUNC * is)).ln(),
        cje: 14.0e-12,
        vje: 0.75,
        mje: 0.33,
        cjc: 4.5e-12,
        vjc: 0.75,
        mjc: 0.33,
        tf: 400e-12,
        tr: 10e-9,
        bf: 600.0,
        br: 0.1,
        vaf: 120.0,
        var: 200.0,
        ikf: 0.05,
        ikr: 0.0,
    }
}

/// 2N3055 power card (TR3) — `main.cpp:299-301`.
pub fn q2n3055() -> BjtParams {
    let is = 4.66e-12;
    BjtParams {
        is,
        vt: 0.02585,
        alpha_f: 0.99723,
        alpha_r: 0.66667,
        vcrit: 0.02585 * (0.02585 / (SQRT2_TRUNC * is)).ln(),
        cje: 580.2e-12,
        vje: 1.2,
        mje: 0.45,
        cjc: 212.1e-12,
        vjc: 0.75,
        mjc: 0.4,
        tf: 80e-9,
        tr: 2.55e-6,
        bf: 360.0,
        br: 2.0,
        vaf: 100.0,
        var: 0.0,
        ikf: 0.25,
        ikr: 0.0,
    }
}

/// BJT instance: terminal node ids + device params + trapezoidal charge-cap companion state
/// (`main.cpp:243-247`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Bjt {
    pub b: i32,
    pub e: i32,
    pub c: i32,
    pub p: BjtParams,
    pub qbe_prev: f64,
    pub ibe_prev: f64,
    pub qbc_prev: f64,
    pub ibc_prev: f64,
}

impl Bjt {
    pub fn new(b: i32, e: i32, c: i32, p: BjtParams) -> Self {
        Bjt { b, e, c, p, qbe_prev: 0.0, ibe_prev: 0.0, qbc_prev: 0.0, ibc_prev: 0.0 }
    }
}

/// Resistor: conductance `g = 1/R` between nodes `a`, `b` (`main.cpp:248`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Res {
    pub a: i32,
    pub b: i32,
    pub g: f64,
}

/// Capacitor with trapezoidal companion: `geq = 2C/dt`, current `i[n] = geq*(dv - dv_prev) -
/// i_prev` (`main.cpp:249-250`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Cap {
    pub a: i32,
    pub b: i32,
    pub c: f64,
    pub geq: f64,
    pub i_prev: f64,
}

/// RBJ biquad lowpass, Direct Form I — decimation anti-aliasing (`main.cpp:22-34`).
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
    pub x1: f64,
    pub x2: f64,
    pub y1: f64,
    pub y2: f64,
}

impl Default for Biquad {
    fn default() -> Self {
        Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }
}

impl Biquad {
    pub fn set_lowpass(&mut self, fc: f64, fs: f64, q: f64) {
        let w0 = 2.0 * std::f64::consts::PI * fc / fs;
        let c = w0.cos();
        let s = w0.sin();
        let al = s / (2.0 * q);
        let a0 = 1.0 + al;
        self.b0 = (1.0 - c) / 2.0 / a0;
        self.b1 = (1.0 - c) / a0;
        self.b2 = self.b0;
        self.a1 = (-2.0 * c) / a0;
        self.a2 = (1.0 - al) / a0;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

// ---------------------------------------------------------------------------
// Fast transcendentals (`main.cpp:99-150`). Range-reduced, |rel err| < ~1e-11 over the domains
// used here, far below the Newton tol — converged node voltages match the std path to ~12 digits.
// The realtime tier runs the fast path (`gExactMath=false`); `EXACT` keeps the std A/B available.
// ---------------------------------------------------------------------------

/// Realtime tier uses fast math. Flip to `true` only for the lossless-validation A/B.
pub const EXACT: bool = false;

/// `exp` via range reduction + 9th-order Taylor + IEEE-754 exponent assembly (`main.cpp:109-128`).
#[inline]
pub fn fast_exp(x: f64) -> f64 {
    const LN2: f64 = 0.6931471805599453;
    const INV_LN2: f64 = 1.4426950408889634;
    let kf = (x * INV_LN2 + 0.5).floor();
    let k = kf as i32;
    let r = x - kf * LN2; // |r| <= ln2/2 ~ 0.3466
    let p = 1.0
        + r * (1.0
            + r * (1.0 / 2.0
                + r * (1.0 / 6.0
                    + r * (1.0 / 24.0
                        + r * (1.0 / 120.0
                            + r * (1.0 / 720.0
                                + r * (1.0 / 5040.0
                                    + r * (1.0 / 40320.0 + r * (1.0 / 362880.0)))))))));
    // 2^k via the IEEE-754 exponent field. MUST guard negative k: reverse-biased junctions give
    // v/Vt ~ -1000 -> k ~ -1339; without the guard, (k+1023) goes negative and the bit pattern
    // wraps to garbage, poisoning the Jacobian.
    if k <= -1023 {
        return 0.0; // underflow: true exp is subnormal/negligible (<1e-308)
    }
    if k >= 1024 {
        return p * f64::INFINITY; // never in domain
    }
    let u: u64 = ((k + 1023) as u64) << 52;
    let pk = f64::from_bits(u);
    p * pk
}

/// `log` via IEEE-754 exponent decomposition + atanh series (`main.cpp:130-142`).
#[inline]
pub fn fast_log(x: f64) -> f64 {
    let bits = x.to_bits();
    let mut e = (((bits >> 52) & 0x7ff) as i32) - 1022; // m in [0.5,1)
    let bits = (bits & 0x000f_ffff_ffff_ffff) | 0x3fe0_0000_0000_0000;
    let mut m = f64::from_bits(bits);
    if m < 0.707_106_781_186_547_6 {
        m *= 2.0;
        e -= 1;
    } // center: m in [~0.707,~1.414)
    let f = m - 1.0;
    let s = f / (2.0 + f);
    let s2 = s * s; // log(m) = 2*atanh(s)
    let poly = 2.0
        * s
        * (1.0 + s2 * (1.0 / 3.0 + s2 * (1.0 / 5.0 + s2 * (1.0 / 7.0 + s2 * (1.0 / 9.0 + s2 / 11.0)))));
    poly + (e as f64) * 0.6931471805599453
}

/// positive-base power via `exp(ex*log(base))`; only called with `base > 0` here (`main.cpp:145`).
#[inline]
pub fn fast_pow_pos(base: f64, ex: f64) -> f64 {
    fast_exp(ex * fast_log(base))
}

/// `EXP` wrapper — std vs fast per `EXACT` (`main.cpp:149`).
#[inline]
pub fn exp_(x: f64) -> f64 {
    if EXACT {
        x.exp()
    } else {
        fast_exp(x)
    }
}

/// `POW` wrapper — std vs fast per `EXACT` (`main.cpp:150`).
#[inline]
pub fn pow_(b: f64, e: f64) -> f64 {
    if EXACT {
        b.powf(e)
    } else {
        fast_pow_pos(b, e)
    }
}

/// Junction depletion charge `Q(v)` and small-signal `C(v)=dQ/dv` for a SPICE pn junction, with
/// the standard `FC=0.5` linear extrapolation above `FC*Vj` so it stays finite forward-biased.
/// Returns `(q, c)` (`main.cpp:154-170`).
#[inline]
pub fn depletion_charge(v: f64, cj0: f64, vj: f64, m: f64) -> (f64, f64) {
    const FC: f64 = 0.5;
    if cj0 <= 0.0 {
        return (0.0, 0.0);
    }
    if v < FC * vj {
        let f = 1.0 - v / vj;
        let c = cj0 * pow_(f, -m);
        let q = cj0 * vj / (1.0 - m) * (1.0 - pow_(f, 1.0 - m));
        (q, c)
    } else {
        let f0 = 1.0 - FC;
        let c0 = cj0 * pow_(f0, -m);
        let q0 = cj0 * vj / (1.0 - m) * (1.0 - pow_(f0, 1.0 - m));
        let dcdv = cj0 * m / vj * pow_(f0, -m - 1.0);
        let d = v - FC * vj;
        let c = c0 + dcdv * d;
        let q = q0 + c0 * d + 0.5 * dcdv * d * d;
        (q, c)
    }
}

/// Diode current + conductance, linearized beyond `vcrit` for Newton stability (SPICE pnjlim
/// spirit). Returns `(i, g)` (`main.cpp:174-187`).
#[inline]
pub fn diode_branch(v: f64, is: f64, vt: f64, vcrit: f64) -> (f64, f64) {
    if v < vcrit {
        let e = exp_(v / vt);
        (is * (e - 1.0), (is / vt) * e)
    } else {
        let ec = exp_(vcrit / vt);
        let icrit = is * (ec - 1.0);
        let gcrit = (is / vt) * ec;
        (icrit + gcrit * (v - vcrit), gcrit) // C1-continuous linear extension
    }
}

/// Gummel-Poon terminal currents (into the device) + node-form Jacobian. With Early effect
/// (VAF/VAR) and high-injection (IKF/IKR); reduces to plain transport when those are disabled
/// (`main.cpp:192-241`). `itf/gtf/itr/gtr` are returned for the diffusion-charge caps.
#[derive(Clone, Copy, Debug, Default)]
pub struct BjtOut {
    pub ib: f64,
    pub ic: f64,
    pub ie: f64,
    pub itf: f64,
    pub gtf: f64,
    pub itr: f64,
    pub gtr: f64,
    pub dib_b: f64,
    pub dib_e: f64,
    pub dib_c: f64,
    pub dic_b: f64,
    pub dic_e: f64,
    pub dic_c: f64,
    pub die_b: f64,
    pub die_e: f64,
    pub die_c: f64,
}

/// Compute Gummel-Poon BJT response (`main.cpp:197-241`). `sqrt` is exact (not fast math),
/// matching the C++ `std::sqrt`.
pub fn compute_bjt(vbe: f64, vbc: f64, p: &BjtParams) -> BjtOut {
    let mut o = BjtOut::default();
    let (itf, gtf) = diode_branch(vbe, p.is, p.vt, p.vcrit); // forward transport + d/dvbe
    let (itr, gtr) = diode_branch(vbc, p.is, p.vt, p.vcrit); // reverse transport + d/dvbc
    o.itf = itf;
    o.gtf = gtf;
    o.itr = itr;
    o.gtr = gtr;

    // base-charge factor qb = (q1/2)(1+sqrt(1+4 q2)); q1=Early, q2=high-injection
    let inv_af = if p.vaf > 0.0 { 1.0 / p.vaf } else { 0.0 };
    let inv_ar = if p.var > 0.0 { 1.0 / p.var } else { 0.0 };
    let mut inv1 = 1.0 - vbc * inv_af - vbe * inv_ar;
    if inv1 < 0.1 {
        inv1 = 0.1; // guard against blow-up in saturation
    }
    let q1 = 1.0 / inv1;
    let dq1_vbe = q1 * q1 * inv_ar; // d(1/inv1)/dvbe = q1^2 * inv_ar
    let dq1_vbc = q1 * q1 * inv_af;
    let inv_kf = if p.ikf > 0.0 { 1.0 / p.ikf } else { 0.0 };
    let inv_kr = if p.ikr > 0.0 { 1.0 / p.ikr } else { 0.0 };
    let q2 = itf * inv_kf + itr * inv_kr;
    let dq2_vbe = gtf * inv_kf;
    let dq2_vbc = gtr * inv_kr;
    let s = (1.0 + 4.0 * q2).sqrt();
    let qb = q1 * (1.0 + s) / 2.0;
    let dqb_vbe = dq1_vbe * (1.0 + s) / 2.0 + q1 * dq2_vbe / s;
    let dqb_vbc = dq1_vbc * (1.0 + s) / 2.0 + q1 * dq2_vbc / s;

    let diff = itf - itr;
    let it = diff / qb;
    let dit_vbe = (gtf * qb - diff * dqb_vbe) / (qb * qb);
    let dit_vbc = (-gtr * qb - diff * dqb_vbc) / (qb * qb);

    let inv_bf = 1.0 / p.bf;
    let inv_br = 1.0 / p.br;
    let ic = it - itr * inv_br;
    let ib = itf * inv_bf + itr * inv_br;
    let ie = -(ic + ib);
    o.ic = ic;
    o.ib = ib;
    o.ie = ie;

    // derivatives wrt vbe, vbc
    let dic_vbe = dit_vbe;
    let dic_vbc = dit_vbc - gtr * inv_br;
    let dib_vbe = gtf * inv_bf;
    let dib_vbc = gtr * inv_br;
    let die_vbe = -(dic_vbe + dib_vbe);
    let die_vbc = -(dic_vbc + dib_vbc);
    // node form: d/dVb = d/dvbe + d/dvbc, d/dVe = -d/dvbe, d/dVc = -d/dvbc
    o.dic_b = dic_vbe + dic_vbc;
    o.dic_e = -dic_vbe;
    o.dic_c = -dic_vbc;
    o.dib_b = dib_vbe + dib_vbc;
    o.dib_e = -dib_vbe;
    o.dib_c = -dib_vbc;
    o.die_b = die_vbe + die_vbc;
    o.die_e = -die_vbe;
    o.die_c = -die_vbc;
    o
}

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
#[derive(Clone, Copy, Debug)]
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
        }
    }
    pub fn fast() -> Self {
        Self {
            predict: true,
            solver: Solver::Schur,
            freeze_caps: true,
            ls_trim: true,
            ..Self::realtime()
        }
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
const MAX_RES: usize = 24;
const MAX_CAPS: usize = 8;

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
}

/// Coupled-circuit MNA + Newton solver. All scratch is fixed-size (no `Vec` on the process path)
/// to satisfy the zero-alloc audio-thread contract.
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

    r_kj_idx: usize,    // R_KJ resistor index (gain knob), for set_rgain
    r_upper_idx: usize, // RV1 upper-leg index, for set_bias
    r_lower_idx: usize, // RV1 lower-leg index

    // coupled inductor companion (transformer, trapezoidal): g = (dt/2) * inv([Lp M; M Ls])
    gl: [[f64; 2]; 2],
    i_hist_p: f64, // primary winding current i[n-1]
    i_hist_s: f64, // secondary winding current i[n-1]
    v_pprev: f64,  // winding voltages at n-1
    v_sprev: f64,

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

    // Newton predictor: cross-sample voltage extrapolation (audio pass only).
    v_pred_prev: [f64; NUM_NODES], // sol(n-2) for the unknown nodes
    predict_primed: bool,          // false until the first extrapolation seed exists
    in_audio_pass: bool,           // off during DC warmup/rail-ramp/settle

    // Freeze-caps: cached iter-0 junction-cap companion per BJT: (cbe, cbc, off_be, off_bc).
    frozen_cap: [(f64, f64, f64, f64); 3],
    frozen_caps_valid: bool,

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
    dec1: Biquad,
    dec2: Biquad,
    sec_in_dc: f64, // output DC offset captured at warmup; subtracted from the tap
    prev_in: f64,   // previous host input sample (linear-interpolation upsampling state)
    last_snapshot: Option<WdfSnapshot>,
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

impl WdfModel {
    #[inline]
    pub fn node_v(&self, id: i32) -> f64 {
        if id < 0 {
            0.0
        } else {
            self.v[id as usize]
        }
    }
    #[inline]
    fn node_v_prev(&self, id: i32) -> f64 {
        if id < 0 {
            0.0
        } else {
            self.v_prev[id as usize]
        }
    }
    /// GND-safe row lookup (`main.cpp:449`).
    #[inline]
    fn ur(&self, id: i32) -> i32 {
        if id < 0 {
            -1
        } else {
            self.urow[id as usize]
        }
    }

    pub fn u(&self) -> usize {
        self.u
    }
    pub fn n_res(&self) -> usize {
        self.n_res
    }
    pub fn n_caps(&self) -> usize {
        self.n_caps
    }
    pub fn n_bjts(&self) -> usize {
        self.bjts.len()
    }
    pub fn urow_of(&self, id: i32) -> i32 {
        self.ur(id)
    }

    /// Build the circuit at the given **internal** sample rate (host × OS), gain shunt `rgain`
    /// (R_KJ ohms), and `bias` (RV1 pot [0,1]) at the realtime tier (all levers off).
    /// Thin wrapper over [`build_with`]; mirrors `BA283::build` (`main.cpp:314-409`).
    pub fn build(internal_sr: f64, rgain: f64, bias: f64) -> Self {
        Self::build_with(QualityConfig::realtime(), internal_sr, rgain, bias)
    }

    /// Build the circuit with an explicit [`QualityConfig`] (tier/lever selection). The topology
    /// is fixed (RB off, U=16); only the solver levers vary. Mirrors `BA283::build` +
    /// `qualityPreset` (`main.cpp:314-409`, `main.cpp:92`).
    pub fn build_with(cfg: QualityConfig, internal_sr: f64, rgain: f64, bias: f64) -> Self {
        use node::*;
        let dt = 1.0 / internal_sr;

        let mut known = [false; NUM_NODES];
        known[N_A as usize] = true;
        known[N_INPUT as usize] = true;
        // RB off: pin the 3 intrinsic-base nodes (V=0) so they're excluded from the unknown map.
        known[N_TR1_BI as usize] = true;
        known[N_TR2_BI as usize] = true;
        known[N_TR3_BI as usize] = true;

        let mut urow = [-1i32; NUM_NODES];
        let mut u = 0usize;
        for i in 0..NUM_NODES {
            urow[i] = if known[i] {
                -1
            } else {
                let r = u as i32;
                u += 1;
                r
            };
        }

        // --- resistors (builder may use Vec; the process path is array-only) ---
        let mut rv: Vec<Res> = Vec::new();
        let push_r = |rv: &mut Vec<Res>, a: i32, b: i32, ohms: f64| rv.push(Res { a, b, g: 1.0 / ohms });
        let (r_upper, r_lower) = rv1_split(bias);
        // TR1 (BC184C) common-emitter input
        push_r(&mut rv, N_C1_MID, N_R1_MID, 1.0); // R_esr_C7 (C1 ESR)
        push_r(&mut rv, N_R1_MID, N_TR1_B, 2.2e3); // R1 input series
        push_r(&mut rv, N_TR1_B, GND, 68.0e3); // R3 base bleed
        push_r(&mut rv, N_RV1_W, N_TR1_B, 56.0e3); // R2 DC-bias feedback
        push_r(&mut rv, N_TR1_E, GND, 1.2e3); // R4 emitter load
        let r_kj_idx = rv.len();
        push_r(&mut rv, N_TR1_E, N_J, rgain); // R_KJ (gain knob)
        push_r(&mut rv, N_CGAIN_MID, GND, 1.0); // R_esr_Cgain
        push_r(&mut rv, N_C4_MID, N_TR1_E, 3.3e3); // R5 AC feedback
        push_r(&mut rv, N_A, N_TR1_C, 33.0e3); // R8 collector load
        // TR2+TR3 compound common-collector pair
        push_r(&mut rv, N_TR2_E, GND, 18.0e3); // R6 TR2 emitter load
        push_r(&mut rv, N_TR3_E, GND, 47.0); // R7 TR3 emitter resistor
        let r_upper_idx = rv.len();
        push_r(&mut rv, N_TR3_E, N_RV1_W, r_upper); // RV1 upper
        let r_lower_idx = rv.len();
        push_r(&mut rv, N_RV1_W, GND, r_lower); // RV1 lower
        // LO1166 + primary Zobel + secondary termination
        push_r(&mut rv, N_F, N_PA, 14.5); // R_pri winding DCR
        push_r(&mut rv, N_SEC_IN, N_SA, 44.0); // R_sec winding DCR
        push_r(&mut rv, N_SEC_IN, GND, 600.0); // R_term (console line in)
        push_r(&mut rv, N_F, N_ZOBEL, 1.5e3); // R_zobel (primary Boucherot snubber)
        // RB off: bases wire directly (no RB resistors).

        let mut res = [Res::default(); MAX_RES];
        let n_res = rv.len();
        res[..n_res].copy_from_slice(&rv);

        // --- caps ---
        let mut cv: Vec<Cap> = Vec::new();
        let push_c = |cv: &mut Vec<Cap>, a: i32, b: i32, farads: f64| {
            cv.push(Cap { a, b, c: farads, geq: 2.0 * farads / dt, i_prev: 0.0 })
        };
        push_c(&mut cv, N_INPUT, N_C1_MID, 10.0e-6); // C1 input coupling
        push_c(&mut cv, N_J, N_CGAIN_MID, 80.0e-6); // Cgain
        push_c(&mut cv, N_F, N_C4_MID, 100.0e-6); // C4 AC-feedback DC block
        push_c(&mut cv, N_TR1_C, GND, 4700.0e-12); // C3 on TR1 collector
        push_c(&mut cv, N_TR1_B, N_TR1_C, 220.0e-12); // C2 Miller across TR1 B-C
        push_c(&mut cv, N_TR1_C, N_F, 330.0e-12); // C5 Miller across TR2 B-C
        push_c(&mut cv, N_ZOBEL, N_A, 0.01e-6); // Zobel cap (primary, to rail)

        let mut caps = [Cap::default(); MAX_CAPS];
        let n_caps = cv.len();
        caps[..n_caps].copy_from_slice(&cv);

        // --- BJTs (RB off: external base directly) ---
        let bc = bc184c();
        let q = q2n3055();
        let bjts = [
            Bjt::new(N_TR1_B, N_TR1_E, N_TR1_C, bc), // TR1
            Bjt::new(N_TR1_C, N_TR2_E, N_F, bc),     // TR2 (base = TR1_C, collector = F)
            Bjt::new(N_TR2_E, N_TR3_E, N_F, q),      // TR3 (base = TR2_E, collector = F)
        ];

        // coupled inductor companion: L = [Lp M; M Ls], M = K*sqrt(Lp*Ls)
        let (lp, ls, k): (f64, f64, f64) = (1.9, 5.36, 0.9995);
        let m = k * (lp * ls).sqrt();
        let det = lp * ls - m * m;
        let l_inv = [[ls / det, -m / det], [-m / det, lp / det]];
        let h = dt * 0.5;
        let gl = [
            [h * l_inv[0][0], h * l_inv[0][1]],
            [h * l_inv[1][0], h * l_inv[1][1]],
        ];

        let mut model = WdfModel {
            v: [0.0; NUM_NODES],
            v_prev: [0.0; NUM_NODES],
            urow,
            u,
            res,
            n_res,
            caps,
            n_caps,
            bjts,
            r_kj_idx,
            r_upper_idx,
            r_lower_idx,
            gl,
            i_hist_p: 0.0,
            i_hist_s: 0.0,
            v_pprev: 0.0,
            v_sprev: 0.0,
            dt,
            newton_tol: cfg.newton_tol,
            max_iters: cfg.max_iters,
            cap_tier: cfg.cap_tier,
            hist: [0.0; NUM_NODES],
            f: [0.0; NUM_NODES],
            delta: [0.0; NUM_NODES],
            rn: [0.0; NUM_NODES],
            j: [0.0; NUM_NODES * NUM_NODES],
            g0: [0.0; NUM_NODES * NUM_NODES],
            predict: cfg.predict,
            solver: cfg.solver,
            freeze_caps: cfg.freeze_caps,
            ls_trim: cfg.ls_trim,
            v_pred_prev: [0.0; NUM_NODES],
            predict_primed: false,
            in_audio_pass: false,
            frozen_cap: [(0.0, 0.0, 0.0, 0.0); 3],
            frozen_caps_valid: false,
            n_idx: [0; NUM_NODES],
            l_idx: [0; NUM_NODES],
            n_sz: 0,
            l_sz: 0,
            schur_ready: false,
            s_ll_fac: [0.0; NUM_NODES * NUM_NODES],
            s_ll_piv: [0; NUM_NODES],
            s_w: [0.0; NUM_NODES * NUM_NODES],
            s_sconst: [0.0; NUM_NODES * NUM_NODES],
            s_anl: [0.0; NUM_NODES * NUM_NODES],
            s_aln: [0.0; NUM_NODES * NUM_NODES],
            s_annlin: [0.0; NUM_NODES * NUM_NODES],
            s_s: [0.0; NUM_NODES * NUM_NODES],
            dec1: Biquad::default(),
            dec2: Biquad::default(),
            sec_in_dc: 0.0,
            prev_in: 0.0,
            last_snapshot: None,
        };
        model.build_g0();
        model.build_schur_partition(); // sets n/l partition; runs first Schur prep if enabled
        model
    }

    /// Construct at a **host** sample rate (internal solve rate = `host_sr * OS`) and configure the
    /// 4th-order Butterworth decimation filters (two biquads, `fc = 0.45*host_sr`, C++ Q values).
    /// Mirrors the C++ offline driver setup (`main.cpp:859-936`).
    pub fn new(host_sr: f64, rgain: f64, bias: f64) -> Self {
        Self::new_with(QualityConfig::realtime(), host_sr, rgain, bias)
    }

    /// Like [`new`], but with an explicit [`QualityConfig`] (tier/lever selection).
    pub fn new_with(cfg: QualityConfig, host_sr: f64, rgain: f64, bias: f64) -> Self {
        let internal_sr = host_sr * OS as f64;
        let mut m = Self::build_with(cfg, internal_sr, rgain, bias);
        let fc = 0.45 * host_sr;
        m.dec1.set_lowpass(fc, internal_sr, 0.54119610);
        m.dec2.set_lowpass(fc, internal_sr, 1.30656296);
        m
    }

    /// Precompute the constant linear conductance matrix G0 (resistors + cap Geq + coupled-inductor
    /// companion). Only BJT entries change per Newton iteration (`main.cpp:413-428`).
    fn build_g0(&mut self) {
        use node::{N_PA, N_SA};
        let u = self.u;
        for x in self.g0[..u * u].iter_mut() {
            *x = 0.0;
        }
        // stamp a symmetric linear conductance G between nodes a, b
        let stamp_lin = |g0: &mut [f64], ra: i32, rb: i32, g: f64| {
            if ra >= 0 {
                g0[ra as usize * u + ra as usize] += g;
                if rb >= 0 {
                    g0[ra as usize * u + rb as usize] -= g;
                }
            }
            if rb >= 0 {
                g0[rb as usize * u + rb as usize] += g;
                if ra >= 0 {
                    g0[rb as usize * u + ra as usize] -= g;
                }
            }
        };
        for i in 0..self.n_res {
            let r = self.res[i];
            let (ra, rb) = (self.ur(r.a), self.ur(r.b));
            stamp_lin(&mut self.g0, ra, rb, r.g);
        }
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let (ra, rb) = (self.ur(c.a), self.ur(c.b));
            stamp_lin(&mut self.g0, ra, rb, c.geq);
        }
        // coupled inductor (asymmetric)
        let r_p = self.ur(N_PA);
        let r_s = self.ur(N_SA);
        if r_p >= 0 {
            self.g0[r_p as usize * u + r_p as usize] += self.gl[0][0];
            if r_s >= 0 {
                self.g0[r_p as usize * u + r_s as usize] += self.gl[0][1];
            }
        }
        if r_s >= 0 {
            if r_p >= 0 {
                self.g0[r_s as usize * u + r_p as usize] += self.gl[1][0];
            }
            self.g0[r_s as usize * u + r_s as usize] += self.gl[1][1];
        }
        // G0 is constant across a sample's Newton iters (changes only on set_bias/set_rgain),
        // so refresh the Schur per-sample factorization here. Guarded until the partition
        // exists (the first build_g0 in build_with runs before build_schur_partition).
        if self.solver == Solver::Schur && self.n_sz > 0 {
            self.prepare_schur();
        }
    }

    /// Update the RV1 split (bias pot) + rebuild G0, preserving all integrator state
    /// (`main.cpp:432-438`).
    pub fn set_bias(&mut self, bias: f64) {
        let (r_upper, r_lower) = rv1_split(bias);
        self.res[self.r_upper_idx].g = 1.0 / r_upper;
        self.res[self.r_lower_idx].g = 1.0 / r_lower;
        self.build_g0();
    }

    /// Update R_KJ (circuit gain) + rebuild G0, preserving integrator state. New vs the C++
    /// reference (which only sets gain at build time); mirrors `set_bias`.
    pub fn set_rgain(&mut self, rgain: f64) {
        self.res[self.r_kj_idx].g = 1.0 / rgain;
        self.build_g0();
    }

    /// TR3 quiescent collector current (mA) at the current operating point (`main.cpp:441-445`).
    pub fn tr3_idle_current_ma(&self) -> f64 {
        let t = &self.bjts[2];
        let vbe = self.node_v(t.b) - self.node_v(t.e);
        let vbc = self.node_v(t.b) - self.node_v(t.c);
        1000.0 * compute_bjt(vbe, vbc, &t.p).ic
    }

    /// Residual 2-norm at current `v` (for line search / convergence), reusing per-sample `hist`.
    /// Must stamp the SAME branches as `newton_iterate`'s F for line-search consistency
    /// (`main.cpp:717-761`).
    fn residual_norm(&mut self, gmin: f64) -> f64 {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
        for i in 0..u {
            self.rn[i] = self.hist[i];
        }
        // resistors
        for i in 0..self.n_res {
            let r = self.res[i];
            let (va, vb) = (self.node_v(r.a), self.node_v(r.b));
            let (ra, rb) = (self.ur(r.a), self.ur(r.b));
            if ra >= 0 {
                self.rn[ra as usize] += r.g * (va - vb);
            }
            if rb >= 0 {
                self.rn[rb as usize] += r.g * (vb - va);
            }
        }
        // caps
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let (va, vb) = (self.node_v(c.a), self.node_v(c.b));
            let (ra, rb) = (self.ur(c.a), self.ur(c.b));
            if ra >= 0 {
                self.rn[ra as usize] += c.geq * (va - vb);
            }
            if rb >= 0 {
                self.rn[rb as usize] += c.geq * (vb - va);
            }
        }
        // coupled inductor
        {
            let vp = self.node_v(N_PA) - self.node_v(N_A);
            let vs = self.node_v(N_SA);
            let (rp, rs) = (self.ur(N_PA), self.ur(N_SA));
            if rp >= 0 {
                self.rn[rp as usize] += self.gl[0][0] * vp + self.gl[0][1] * vs;
            }
            if rs >= 0 {
                self.rn[rs as usize] += self.gl[1][0] * vp + self.gl[1][1] * vs;
            }
        }
        // BJTs (transport + charge caps — must match newton_iterate's F)
        let k2 = 2.0 / self.dt;
        for bi in 0..self.bjts.len() {
            let t = self.bjts[bi];
            let vbe = self.node_v(t.b) - self.node_v(t.e);
            let vbc = self.node_v(t.b) - self.node_v(t.c);
            let o = compute_bjt(vbe, vbc, &t.p);
            let (rb, re, rc) = (self.ur(t.b), self.ur(t.e), self.ur(t.c));
            if rb >= 0 {
                self.rn[rb as usize] += o.ib;
            }
            if rc >= 0 {
                self.rn[rc as usize] += o.ic;
            }
            if re >= 0 {
                self.rn[re as usize] += o.ie;
            }
            if self.cap_tier != CapTier::Off {
                let tf = if self.cap_tier == CapTier::Full { t.p.tf } else { 0.0 };
                let tr = if self.cap_tier == CapTier::Full { t.p.tr } else { 0.0 };
                // Match newton_iterate's F: use the frozen companion when active (`main.cpp:960`).
                let (qbe, qbc) = if self.freeze_caps && self.frozen_caps_valid {
                    let fc = self.frozen_cap[bi];
                    (fc.2 + fc.0 * vbe, fc.3 + fc.1 * vbc)
                } else {
                    let (qbe_dep, _) = depletion_charge(vbe, t.p.cje, t.p.vje, t.p.mje);
                    let (qbc_dep, _) = depletion_charge(vbc, t.p.cjc, t.p.vjc, t.p.mjc);
                    (qbe_dep + tf * o.itf, qbc_dep + tr * o.itr)
                };
                let ibe = k2 * (qbe - t.qbe_prev) - t.ibe_prev;
                let ibc = k2 * (qbc - t.qbc_prev) - t.ibc_prev;
                if rb >= 0 {
                    self.rn[rb as usize] += ibe + ibc;
                }
                if re >= 0 {
                    self.rn[re as usize] -= ibe;
                }
                if rc >= 0 {
                    self.rn[rc as usize] -= ibc;
                }
            }
        }
        if gmin != 0.0 {
            for i in 0..NUM_NODES {
                let r = self.ur(i as i32);
                if r >= 0 {
                    self.rn[r as usize] += gmin * self.v[i];
                }
            }
        }
        let mut n = 0.0;
        for i in 0..u {
            n += self.rn[i] * self.rn[i];
        }
        n.sqrt()
    }

    /// Dense Gaussian elimination with index-swap partial pivoting. Solves `A x = b` in place
    /// (`x` in `b`). `A` is accessed through the `row[]` permutation; `b` is swapped physically —
    /// the asymmetry is bit-identical to row-swap (`main.cpp:764-794`, commit 42b51ef).
    fn gauss_solve(a: &mut [f64], b: &mut [f64], n: usize) -> bool {
        let mut row = [0usize; NUM_NODES];
        for (i, slot) in row.iter_mut().enumerate().take(n) {
            *slot = i;
        }
        for col in 0..n {
            let mut piv = col;
            let mut best = a[row[col] * n + col].abs();
            for r in (col + 1)..n {
                let val = a[row[r] * n + col].abs();
                if val > best {
                    best = val;
                    piv = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if piv != col {
                row.swap(col, piv);
                b.swap(col, piv);
            }
            let diag = a[row[col] * n + col];
            for r in (col + 1)..n {
                let f = a[row[r] * n + col] / diag;
                if f == 0.0 {
                    continue;
                }
                for c in col..n {
                    a[row[r] * n + c] -= f * a[row[col] * n + c];
                }
                b[r] -= f * b[col];
            }
        }
        for r in (0..n).rev() {
            let mut s = b[r];
            for c in (r + 1)..n {
                s -= a[row[r] * n + c] * b[c];
            }
            b[r] = s / a[row[r] * n + r];
        }
        true
    }

    /// Reusable LU (partial pivot, index-swap) for the constant per-sample A_LL block: factor
    /// once, apply many times. Stores L/U in-place (row-permuted) + the pivot order so any RHS
    /// can be solved without refactoring (`main.cpp:1026-1047`).
    fn lu_factor(a: &mut [f64], piv: &mut [usize], n: usize) -> bool {
        for (i, slot) in piv.iter_mut().enumerate().take(n) {
            *slot = i;
        }
        for col in 0..n {
            let mut p = col;
            let mut best = a[piv[col] * n + col].abs();
            for r in (col + 1)..n {
                let val = a[piv[r] * n + col].abs();
                if val > best {
                    best = val;
                    p = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if p != col {
                piv.swap(col, p);
            }
            let diag = a[piv[col] * n + col];
            for r in (col + 1)..n {
                let f = a[piv[r] * n + col] / diag;
                a[piv[r] * n + col] = f; // store multiplier (L factor)
                if f == 0.0 {
                    continue;
                }
                for c in (col + 1)..n {
                    a[piv[r] * n + c] -= f * a[piv[col] * n + c];
                }
            }
        }
        true
    }

    /// Solve `A x = b` in place (`x` in `b`) using LU factors + pivot order from [`lu_factor`]
    /// (`main.cpp:1049-1062`). `y` scratch is a stack array (no heap alloc).
    fn lu_solve(a: &[f64], piv: &[usize], n: usize, b: &mut [f64]) {
        let mut y = [0.0f64; NUM_NODES];
        for r in 0..n {
            let mut s = b[piv[r]];
            for c in 0..r {
                s -= a[piv[r] * n + c] * y[c];
            }
            y[r] = s; // unit-diagonal L
        }
        for r in (0..n).rev() {
            let mut s = y[r];
            for c in (r + 1)..n {
                s -= a[piv[r] * n + c] * b[c];
            }
            b[r] = s / a[piv[r] * n + r];
        }
    }

    /// Build the Schur N/L partition once (at construction). N = every unknown row that a BJT
    /// terminal (and thus its charge cap) stamps; L = the remaining linear-only unknowns. Derived
    /// from the actual `bjts[]` terminal ids so it always matches the stamping (`main.cpp:483-527`).
    fn build_schur_partition(&mut self) {
        let mut in_n = [false; NUM_NODES];
        for t in self.bjts.iter() {
            for nd in [t.b, t.e, t.c] {
                if nd >= 0 && self.urow[nd as usize] >= 0 {
                    in_n[nd as usize] = true;
                }
            }
        }
        self.n_sz = 0;
        for nd in 0..NUM_NODES {
            if !in_n[nd] {
                continue;
            }
            let r = self.urow[nd];
            if r < 0 {
                continue;
            }
            self.n_idx[self.n_sz] = r as usize;
            self.n_sz += 1;
        }
        let mut is_n_row = [false; NUM_NODES];
        for i in 0..self.n_sz {
            is_n_row[self.n_idx[i]] = true;
        }
        self.l_sz = 0;
        for r in 0..self.u {
            if is_n_row[r] {
                continue;
            }
            self.l_idx[self.l_sz] = r;
            self.l_sz += 1;
        }
        self.schur_ready = false;
        // First-time prep (build_with's initial build_g0 ran before the partition existed).
        if self.solver == Solver::Schur {
            self.prepare_schur();
        }
    }

    /// Per-sample Schur setup (valid only on the `gmin == 0` path). Slices the CONSTANT linear
    /// blocks out of G0, LU-factors A_LL, and precomputes `W = A_LL^{-1} A_LN` and
    /// `S_const = A_NN_lin - A_NL W`. Sets `schur_ready`; returns false (→ dense fallback) if
    /// A_LL is singular (`main.cpp:533-568`).
    fn prepare_schur(&mut self) -> bool {
        self.schur_ready = false;
        let u = self.u;
        let n = self.n_sz;
        let l = self.l_sz;
        for i in 0..n {
            for j in 0..n {
                self.s_annlin[i * n + j] = self.g0[self.n_idx[i] * u + self.n_idx[j]];
            }
        }
        for i in 0..n {
            for j in 0..l {
                self.s_anl[i * l + j] = self.g0[self.n_idx[i] * u + self.l_idx[j]];
            }
        }
        for i in 0..l {
            for j in 0..n {
                self.s_aln[i * n + j] = self.g0[self.l_idx[i] * u + self.n_idx[j]];
            }
        }
        for i in 0..l {
            for j in 0..l {
                self.s_ll_fac[i * l + j] = self.g0[self.l_idx[i] * u + self.l_idx[j]];
            }
        }
        if !Self::lu_factor(&mut self.s_ll_fac, &mut self.s_ll_piv, l) {
            return false;
        }
        // W = A_LL^{-1} A_LN  (solve A_LL * W[:,c] = A_LN[:,c] for each N column)
        let mut col = [0.0f64; NUM_NODES];
        for c in 0..n {
            for i in 0..l {
                col[i] = self.s_aln[i * n + c];
            }
            Self::lu_solve(&self.s_ll_fac, &self.s_ll_piv, l, &mut col);
            for i in 0..l {
                self.s_w[i * n + c] = col[i];
            }
        }
        // S_const = A_NN_lin - A_NL W
        for i in 0..n {
            for j in 0..n {
                let mut acc = 0.0;
                for k in 0..l {
                    acc += self.s_anl[i * l + k] * self.s_w[k * n + j];
                }
                self.s_sconst[i * n + j] = self.s_annlin[i * n + j] - acc;
            }
        }
        self.schur_ready = true;
        true
    }

    /// Schur static-block solve of `J delta = -F`, reusing the per-sample A_LL factorization +
    /// `S_const`. Reads the freshly built full `J` (`self.j`) and the RHS `-F` already scattered
    /// into `self.delta`, and writes the solution back into `self.delta`. Only the NxN complement
    /// is factored per iter (full partial pivot via [`gauss_solve`]). Caller guarantees
    /// `gmin == 0` and `schur_ready`. Returns false if the complement is singular (→ dense
    /// fallback; `self.delta` is left as `-F`, untouched) (`main.cpp:1069-1104`).
    fn schur_solve(&mut self) -> bool {
        let u = self.u;
        let n = self.n_sz;
        let l = self.l_sz;
        let mut b_n = [0.0f64; NUM_NODES];
        let mut y = [0.0f64; NUM_NODES];
        for i in 0..n {
            b_n[i] = self.delta[self.n_idx[i]];
        }
        for i in 0..l {
            y[i] = self.delta[self.l_idx[i]]; // y starts as bL
        }
        // S = S_const + (J_NN - A_NN_lin)  (only the nonlinear block changes per iter)
        for i in 0..n {
            let gi = self.n_idx[i];
            for j in 0..n {
                let jnn = self.j[gi * u + self.n_idx[j]];
                self.s_s[i * n + j] = self.s_sconst[i * n + j] + (jnn - self.s_annlin[i * n + j]);
            }
        }
        // y = A_LL^{-1} bL (reuse per-sample factorization)
        Self::lu_solve(&self.s_ll_fac, &self.s_ll_piv, l, &mut y);
        // b~N = bN - A_NL y
        let mut x_n = [0.0f64; NUM_NODES];
        for i in 0..n {
            let mut acc = 0.0;
            for k in 0..l {
                acc += self.s_anl[i * l + k] * y[k];
            }
            x_n[i] = b_n[i] - acc;
        }
        // solve S xN = b~N with full partial pivoting (S can be ill-conditioned)
        if !Self::gauss_solve(&mut self.s_s, &mut x_n, n) {
            return false;
        }
        // xL = y - W xN
        for i in 0..l {
            let mut acc = 0.0;
            for k in 0..n {
                acc += self.s_w[i * n + k] * x_n[k];
            }
            y[i] -= acc;
        }
        // scatter back into delta
        for i in 0..n {
            self.delta[self.n_idx[i]] = x_n[i];
        }
        for i in 0..l {
            self.delta[self.l_idx[i]] = y[i];
        }
        true
    }

    /// One Newton solve (up to `max_iters`) at the given `gmin` regularization. Returns iters used.
    /// Honors the active levers: `ls_trim` (deferred line search), `freeze_caps` (frozen iter-0
    /// cap companion), and `solver` (dense vs Schur) (`main.cpp:701-927`).
    fn newton_iterate(&mut self, gmin: f64) -> usize {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
        let mut iters_used = 0;

        // --freeze-caps: reset per newton_iterate call (NOT once per sample), so each
        // gmin-stepping homotopy pass freezes its cap companion at ITS own iter-0 operating
        // point rather than reusing a stale one from the gmin=0 attempt (`main.cpp:712`).
        self.frozen_caps_valid = false;

        // Deferred-acceptance line-search state (only used when ls_trim). Instead of verifying
        // each Newton step with its own residual_norm call, take the full step optimistically and
        // let the NEXT iter's mandatory F build double as verification; if the residual rose,
        // retroactively restore and run a real backtracking line search (`main.cpp:714-723`).
        let use_trim = self.ls_trim;
        let mut prev_norm_f = f64::INFINITY; // guarded by `provisional`, never read on iter 0
        let mut provisional = false;
        let mut v_before_step = [0.0f64; NUM_NODES];
        let mut last_delta = [0.0f64; NUM_NODES]; // first `u` entries valid

        let mut iter = 0;
        while iter < self.max_iters {
            iters_used = iter + 1;
            // J starts as the constant linear conductance matrix; F starts at history.
            self.j[..u * u].copy_from_slice(&self.g0[..u * u]);
            self.f[..u].copy_from_slice(&self.hist[..u]);

            // linear branch residuals into F only (their J entries are already in G0)
            for i in 0..self.n_res {
                let r = self.res[i];
                let d = r.g * (self.node_v(r.a) - self.node_v(r.b));
                let (ra, rb) = (self.ur(r.a), self.ur(r.b));
                if ra >= 0 {
                    self.f[ra as usize] += d;
                }
                if rb >= 0 {
                    self.f[rb as usize] -= d;
                }
            }
            for i in 0..self.n_caps {
                let c = self.caps[i];
                let d = c.geq * (self.node_v(c.a) - self.node_v(c.b));
                let (ra, rb) = (self.ur(c.a), self.ur(c.b));
                if ra >= 0 {
                    self.f[ra as usize] += d;
                }
                if rb >= 0 {
                    self.f[rb as usize] -= d;
                }
            }
            {
                // coupled inductor residual (F only)
                let vp = self.node_v(N_PA) - self.node_v(N_A);
                let vs = self.node_v(N_SA);
                let (rp, rs) = (self.ur(N_PA), self.ur(N_SA));
                if rp >= 0 {
                    self.f[rp as usize] += self.gl[0][0] * vp + self.gl[0][1] * vs;
                }
                if rs >= 0 {
                    self.f[rs as usize] += self.gl[1][0] * vp + self.gl[1][1] * vs;
                }
            }

            // BJTs (Gummel-Poon transport form). Currents flow INTO the device.
            let k2 = 2.0 / self.dt;
            for bi in 0..self.bjts.len() {
                let t = self.bjts[bi];
                let vbe = self.node_v(t.b) - self.node_v(t.e);
                let vbc = self.node_v(t.b) - self.node_v(t.c);
                let o = compute_bjt(vbe, vbc, &t.p);
                let (rb, re, rc) = (self.urow[t.b as usize], self.urow[t.e as usize], self.urow[t.c as usize]);
                if rb >= 0 {
                    let rb = rb as usize;
                    self.f[rb] += o.ib;
                    self.j[rb * u + rb] += o.dib_b;
                    if re >= 0 {
                        self.j[rb * u + re as usize] += o.dib_e;
                    }
                    if rc >= 0 {
                        self.j[rb * u + rc as usize] += o.dib_c;
                    }
                }
                if re >= 0 {
                    let re = re as usize;
                    self.f[re] += o.ie;
                    if rb >= 0 {
                        self.j[re * u + rb as usize] += o.die_b;
                    }
                    self.j[re * u + re] += o.die_e;
                    if rc >= 0 {
                        self.j[re * u + rc as usize] += o.die_c;
                    }
                }
                if rc >= 0 {
                    let rc = rc as usize;
                    self.f[rc] += o.ic;
                    if rb >= 0 {
                        self.j[rc * u + rb as usize] += o.dic_b;
                    }
                    if re >= 0 {
                        self.j[rc * u + re as usize] += o.dic_e;
                    }
                    self.j[rc * u + rc] += o.dic_c;
                }

                // Charge-storage caps (depletion + diffusion), trapezoidal companion.
                if self.cap_tier != CapTier::Off {
                    let tf = if self.cap_tier == CapTier::Full { t.p.tf } else { 0.0 };
                    let tr = if self.cap_tier == CapTier::Full { t.p.tr } else { 0.0 };
                    // --freeze-caps: iters>=1 reuse the frozen linear-cap companion (Q = off +
                    // C*v) with the iter-0 C — no depletion_charge/POW. Iter 0 computes the full
                    // depletion charge and stores the companion (`main.cpp:782-798`).
                    let (qbe, cbe, qbc, cbc) = if self.freeze_caps && self.frozen_caps_valid {
                        let fc = self.frozen_cap[bi];
                        (fc.2 + fc.0 * vbe, fc.0, fc.3 + fc.1 * vbc, fc.1)
                    } else {
                        let (qbe_dep, cbe_dep) = depletion_charge(vbe, t.p.cje, t.p.vje, t.p.mje);
                        let (qbc_dep, cbc_dep) = depletion_charge(vbc, t.p.cjc, t.p.vjc, t.p.mjc);
                        let qbe = qbe_dep + tf * o.itf;
                        let cbe = cbe_dep + tf * o.gtf;
                        let qbc = qbc_dep + tr * o.itr;
                        let cbc = cbc_dep + tr * o.gtr;
                        if self.freeze_caps {
                            self.frozen_cap[bi] = (cbe, cbc, qbe - cbe * vbe, qbc - cbc * vbc);
                        }
                        (qbe, cbe, qbc, cbc)
                    };
                    let ibe = k2 * (qbe - t.qbe_prev) - t.ibe_prev; // b->e
                    let gbe = k2 * cbe;
                    let ibc = k2 * (qbc - t.qbc_prev) - t.ibc_prev; // b->c
                    let gbc = k2 * cbc;
                    if rb >= 0 {
                        let rb = rb as usize;
                        self.f[rb] += ibe + ibc;
                        self.j[rb * u + rb] += gbe + gbc;
                        if re >= 0 {
                            self.j[rb * u + re as usize] -= gbe;
                        }
                        if rc >= 0 {
                            self.j[rb * u + rc as usize] -= gbc;
                        }
                    }
                    if re >= 0 {
                        let re = re as usize;
                        self.f[re] -= ibe;
                        self.j[re * u + re] += gbe;
                        if rb >= 0 {
                            self.j[re * u + rb as usize] -= gbe;
                        }
                    }
                    if rc >= 0 {
                        let rc = rc as usize;
                        self.f[rc] -= ibc;
                        self.j[rc * u + rc] += gbc;
                        if rb >= 0 {
                            self.j[rc * u + rb as usize] -= gbc;
                        }
                    }
                }
            }

            // --freeze-caps: the iter that ran the full depletion_charge path has now populated
            // frozen_cap[] for every BJT; subsequent iters reuse the frozen C (`main.cpp:819`).
            if self.freeze_caps {
                self.frozen_caps_valid = true;
            }

            // gmin homotopy: conductance from each node to ground
            if gmin != 0.0 {
                for i in 0..NUM_NODES {
                    let r = self.urow[i];
                    if r >= 0 {
                        let r = r as usize;
                        self.f[r] += gmin * self.v[i];
                        self.j[r * u + r] += gmin;
                    }
                }
            }

            // 2-norm of residual; Newton step is a descent direction for this. This F is
            // evaluated at the current v, so in trim mode it also verifies the previous iter's
            // optimistic full step.
            let mut norm_f = 0.0;
            for i in 0..u {
                norm_f += self.f[i] * self.f[i];
            }
            norm_f = norm_f.sqrt();
            if norm_f < self.newton_tol {
                break;
            }

            // Trim mode: retroactively reject a provisional step that increased the residual,
            // falling back to a real backtracking line search from the saved iterate. (Eager mode
            // never sets `provisional`, so this is skipped.) (`main.cpp:839-856`)
            if use_trim && provisional && norm_f > prev_norm_f {
                let mut lambda = 1.0;
                let mut accepted = 0.0;
                for _ls in 0..12 {
                    self.v.copy_from_slice(&v_before_step);
                    for i in 0..NUM_NODES {
                        let r = self.urow[i];
                        if r >= 0 {
                            self.v[i] += lambda * last_delta[r as usize];
                        }
                    }
                    let tn = self.residual_norm(gmin);
                    if tn < prev_norm_f {
                        accepted = lambda;
                        break;
                    }
                    lambda *= 0.5;
                }
                if accepted == 0.0 {
                    // no descent: restore and stop
                    self.v.copy_from_slice(&v_before_step);
                    break;
                }
                provisional = false; // corrected; rebuild + re-verify next iter
                iter += 1;
                continue;
            }

            for i in 0..u {
                self.delta[i] = -self.f[i];
            }
            // Solve J*delta = -F. Schur only on the gmin==0 hot path (gmin would perturb A_LL and
            // invalidate the per-sample factorization); falls back to dense if Schur prep failed
            // or the complement is singular this iter (`main.cpp:862-867`).
            let solved = if self.solver == Solver::Schur && gmin == 0.0 && self.schur_ready {
                self.schur_solve() || Self::gauss_solve(&mut self.j, &mut self.delta, u)
            } else {
                Self::gauss_solve(&mut self.j, &mut self.delta, u)
            };
            if !solved {
                break;
            }

            // Bound step magnitude, preserving the Newton direction (ill-conditioned J).
            let mut max_abs = 0.0f64;
            for i in 0..u {
                max_abs = max_abs.max(self.delta[i].abs());
            }
            const MAX_STEP: f64 = 2.0; // volts
            if max_abs > MAX_STEP {
                let s = MAX_STEP / max_abs;
                for i in 0..u {
                    self.delta[i] *= s;
                }
            }

            if use_trim {
                // Save the iterate + step, take the full step optimistically, and defer
                // verification to the next iter's F build. No residual_norm call on the common
                // (monotone-converging) path (`main.cpp:878-892`).
                v_before_step.copy_from_slice(&self.v);
                last_delta[..u].copy_from_slice(&self.delta[..u]);
                prev_norm_f = norm_f;
                for i in 0..NUM_NODES {
                    let r = self.urow[i];
                    if r >= 0 {
                        self.v[i] += self.delta[r as usize];
                    }
                }
                provisional = true;
                let mut maxd = 0.0f64;
                for i in 0..u {
                    maxd = maxd.max(self.delta[i].abs());
                }
                if maxd < 1e-9 {
                    break;
                }
                iter += 1;
                continue;
            }

            // Backtracking line search on 2-norm; scale whole vector (preserve direction).
            let mut v_save = [0.0f64; NUM_NODES];
            v_save.copy_from_slice(&self.v);
            let mut lambda = 1.0;
            let mut accepted = 0.0;
            for _ls in 0..12 {
                self.v.copy_from_slice(&v_save);
                for i in 0..NUM_NODES {
                    let r = self.urow[i];
                    if r >= 0 {
                        self.v[i] += lambda * self.delta[r as usize];
                    }
                }
                let tn = self.residual_norm(gmin);
                if tn < norm_f {
                    accepted = lambda;
                    break;
                }
                lambda *= 0.5;
            }
            if accepted == 0.0 {
                // no descent found: stay put, stop iterating
                self.v.copy_from_slice(&v_save);
                break;
            }

            let mut maxd = 0.0f64;
            for i in 0..u {
                maxd = maxd.max((accepted * self.delta[i]).abs());
            }
            if maxd < 1e-9 {
                break;
            }
            iter += 1;
        }
        // Trim mode: only when the loop hit the maxIters cap is the final optimistic step left
        // unverified (no following F build checked it). Verify once and roll back to the pre-step
        // iterate if the residual rose, so we never return a worse point than the step started
        // from (the eager path guarantees this per-step) (`main.cpp:922-925`).
        if use_trim && provisional && iter == self.max_iters {
            if self.residual_norm(gmin) > prev_norm_f {
                self.v.copy_from_slice(&v_before_step);
            }
        }
        iters_used
    }

    /// Solve one sample. `v[N_A]`, `v[N_INPUT]` must be set before calling (`main.cpp:452-516`).
    pub fn process(&mut self) {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
        // Newton predictor: seed the solve with a 1st-order linear extrapolation of the last two
        // converged internal samples (2*sol(n-1) - sol(n-2)) for UNKNOWN nodes only. Same fixed
        // point + tol => identical converged v, just fewer iters. Off during DC warmup/rail-ramp/
        // settle (in_audio_pass false), where transient swings would make it overshoot
        // (`main.cpp:621-633`). Modifies only `v` (the guess); `hist` below reads `v_prev`.
        if self.predict && self.in_audio_pass {
            if !self.predict_primed {
                self.v_pred_prev = self.v; // remember sol(n-1); no extrapolation yet
                self.predict_primed = true;
            } else {
                for i in 0..NUM_NODES {
                    if self.urow[i] < 0 {
                        continue; // skip known/pinned nodes (same mask as the solver)
                    }
                    let cur = self.v[i]; // sol(n-1)
                    self.v[i] = 2.0 * self.v[i] - self.v_pred_prev[i];
                    self.v_pred_prev[i] = cur; // becomes sol(n-2) for the next call
                }
            }
        }
        // Per-sample constant history offsets (do not depend on v[n]).
        for i in 0..u {
            self.hist[i] = 0.0;
        }
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let ih = c.geq * (self.node_v_prev(c.a) - self.node_v_prev(c.b)) + c.i_prev;
            let (ra, rb) = (self.ur(c.a), self.ur(c.b));
            if ra >= 0 {
                self.hist[ra as usize] -= ih;
            }
            if rb >= 0 {
                self.hist[rb as usize] += ih;
            }
        }
        // trapezoidal inductor history: i[n-1] + g*v[n-1]
        let ind_hist_p = self.i_hist_p + self.gl[0][0] * self.v_pprev + self.gl[0][1] * self.v_sprev;
        let ind_hist_s = self.i_hist_s + self.gl[1][0] * self.v_pprev + self.gl[1][1] * self.v_sprev;
        let (rp, rs) = (self.ur(N_PA), self.ur(N_SA));
        if rp >= 0 {
            self.hist[rp as usize] += ind_hist_p;
        }
        if rs >= 0 {
            self.hist[rs as usize] += ind_hist_s;
        }

        self.newton_iterate(0.0);
        let resid = self.residual_norm(0.0);
        if resid > 1e-3 {
            // hard sample: gmin-stepping homotopy
            for g in [1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 0.0] {
                self.newton_iterate(g);
            }
        }

        // update cap currents (trapezoidal) for next sample, using old v_prev
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let dv = self.node_v(c.a) - self.node_v(c.b);
            let dv_prev = self.node_v_prev(c.a) - self.node_v_prev(c.b);
            self.caps[i].i_prev = c.geq * (dv - dv_prev) - c.i_prev;
        }
        // update BJT charge-cap state (trapezoidal) for next sample
        if self.cap_tier != CapTier::Off {
            let k2 = 2.0 / self.dt;
            for bi in 0..self.bjts.len() {
                let t = self.bjts[bi];
                let tf = if self.cap_tier == CapTier::Full { t.p.tf } else { 0.0 };
                let tr = if self.cap_tier == CapTier::Full { t.p.tr } else { 0.0 };
                let vbe = self.node_v(t.b) - self.node_v(t.e);
                let vbc = self.node_v(t.b) - self.node_v(t.c);
                let (i_be, _) = diode_branch(vbe, t.p.is, t.p.vt, t.p.vcrit);
                let (i_bc, _) = diode_branch(vbc, t.p.is, t.p.vt, t.p.vcrit);
                let (qbe_dep, _) = depletion_charge(vbe, t.p.cje, t.p.vje, t.p.mje);
                let (qbc_dep, _) = depletion_charge(vbc, t.p.cjc, t.p.vjc, t.p.mjc);
                let qbe = qbe_dep + tf * i_be;
                let qbc = qbc_dep + tr * i_bc;
                let bjt = &mut self.bjts[bi];
                bjt.ibe_prev = k2 * (qbe - bjt.qbe_prev) - bjt.ibe_prev;
                bjt.qbe_prev = qbe;
                bjt.ibc_prev = k2 * (qbc - bjt.qbc_prev) - bjt.ibc_prev;
                bjt.qbc_prev = qbc;
            }
        }
        // update inductor winding currents (trapezoidal) for next sample
        let vp = self.node_v(N_PA) - self.node_v(N_A);
        let vs = self.node_v(N_SA);
        self.i_hist_p += self.gl[0][0] * (vp + self.v_pprev) + self.gl[0][1] * (vs + self.v_sprev);
        self.i_hist_s += self.gl[1][0] * (vp + self.v_pprev) + self.gl[1][1] * (vs + self.v_sprev);
        self.v_pprev = vp;
        self.v_sprev = vs;

        for i in 0..NUM_NODES {
            self.v_prev[i] = self.v[i];
        }
    }

    fn internal_sr(&self) -> f64 {
        1.0 / self.dt
    }

    /// Idle settle with tap-delta early-exit (`main.cpp:874-886`), capped at `seconds`.
    fn idle_settle(&mut self, seconds: f64) {
        let internal_sr = self.internal_sr();
        let chunk = (internal_sr * 0.25) as usize;
        let idle_max = (internal_sr * seconds) as usize;
        let mut done = 0;
        let mut prev_tap = 1e9;
        while done < idle_max {
            for _ in 0..chunk {
                self.process();
            }
            done += chunk;
            let tap = self.v[node::N_SEC_IN as usize];
            if (tap - prev_tap).abs() < 1e-4 {
                break; // settled
            }
            prev_tap = tap;
        }
    }

    /// Soft-start from cold: ramp the +24V rail (1 s homotopy), let the DC feedback loop self-bias
    /// (idle-settle, early-exit), then capture the output DC offset. Mirrors `main.cpp:865-916`
    /// MINUS auto-bias — the bias is a direct param here, not bisected to a target idle current.
    pub fn warmup(&mut self) {
        let internal_sr = self.internal_sr();
        let ramp = internal_sr as usize; // 1 s
        self.v[node::N_INPUT as usize] = 0.0;
        for i in 0..ramp {
            self.v[node::N_A as usize] = 24.0 * (i as f64) / (ramp as f64);
            self.process();
        }
        self.v[node::N_A as usize] = 24.0;
        self.idle_settle(4.0);
        self.sec_in_dc = self.v[node::N_SEC_IN as usize];
    }

    /// Process one host-rate sample: linear-upsample to `OS` subsamples, Newton-solve each, decimate
    /// the DC-corrected tap through the two Butterworth biquads (`main.cpp:942-953`).
    #[inline]
    pub fn process_host_sample(&mut self, x: f32) -> f32 {
        // Audio pass: enable the Newton predictor (idempotent; warmup runs with it off).
        self.in_audio_pass = true;
        let cur_in = x as f64;
        let mut decimated = 0.0;
        for j in 0..OS {
            let frac = (j + 1) as f64 / OS as f64; // linear-interpolate upsample
            self.v[node::N_INPUT as usize] = self.prev_in + (cur_in - self.prev_in) * frac;
            self.process();
            let y = self
                .dec2
                .process(self.dec1.process(self.v[node::N_SEC_IN as usize] - self.sec_in_dc));
            decimated = y; // keep last filtered subsample
        }
        self.prev_in = cur_in;
        decimated as f32
    }

    pub fn sec_in_dc(&self) -> f64 {
        self.sec_in_dc
    }

    /// Capture the current settled state (`main.cpp` integrator + DC offset). Pure.
    pub fn snapshot(&self) -> WdfSnapshot {
        let mut caps_i_prev = [0.0; MAX_CAPS];
        for i in 0..self.n_caps {
            caps_i_prev[i] = self.caps[i].i_prev;
        }
        let mut bjt_state = [(0.0, 0.0, 0.0, 0.0); 3];
        for (i, b) in self.bjts.iter().enumerate() {
            bjt_state[i] = (b.qbe_prev, b.ibe_prev, b.qbc_prev, b.ibc_prev);
        }
        WdfSnapshot {
            v: self.v,
            v_prev: self.v_prev,
            caps_i_prev,
            bjt_state,
            i_hist_p: self.i_hist_p,
            i_hist_s: self.i_hist_s,
            v_pprev: self.v_pprev,
            v_sprev: self.v_sprev,
            sec_in_dc: self.sec_in_dc,
        }
    }

    /// Restore integrator state from a snapshot; clears decimation-biquad + upsample state (their
    /// transient is sub-millisecond).
    pub fn restore(&mut self, s: &WdfSnapshot) {
        self.v = s.v;
        self.v_prev = s.v_prev;
        for i in 0..self.n_caps {
            self.caps[i].i_prev = s.caps_i_prev[i];
        }
        for (i, b) in self.bjts.iter_mut().enumerate() {
            b.qbe_prev = s.bjt_state[i].0;
            b.ibe_prev = s.bjt_state[i].1;
            b.qbc_prev = s.bjt_state[i].2;
            b.ibc_prev = s.bjt_state[i].3;
        }
        self.i_hist_p = s.i_hist_p;
        self.i_hist_s = s.i_hist_s;
        self.v_pprev = s.v_pprev;
        self.v_sprev = s.v_sprev;
        self.sec_in_dc = s.sec_in_dc;
        // Re-prime the predictor on the first audio sample after a restore (the warmup snapshot
        // carries no extrapolation history; a stale seed would overshoot sample 0).
        self.predict_primed = false;
        self.in_audio_pass = false;
        self.clear_driver_state();
    }

    /// Store the current state as the snapshot reset() restores to (call after `warmup`).
    pub fn save_snapshot(&mut self) {
        self.last_snapshot = Some(self.snapshot());
    }

    fn clear_driver_state(&mut self) {
        self.prev_in = 0.0;
        for bq in [&mut self.dec1, &mut self.dec2] {
            bq.x1 = 0.0;
            bq.x2 = 0.0;
            bq.y1 = 0.0;
            bq.y2 = 0.0;
        }
    }

    /// Restore to the last snapshot if present, else zero all state. Clears the decimation filters.
    pub fn reset(&mut self) {
        if let Some(s) = self.last_snapshot {
            self.restore(&s);
        } else {
            self.v = [0.0; NUM_NODES];
            self.v_prev = [0.0; NUM_NODES];
            for i in 0..self.n_caps {
                self.caps[i].i_prev = 0.0;
            }
            for b in self.bjts.iter_mut() {
                b.qbe_prev = 0.0;
                b.ibe_prev = 0.0;
                b.qbc_prev = 0.0;
                b.ibc_prev = 0.0;
            }
            self.i_hist_p = 0.0;
            self.i_hist_s = 0.0;
            self.v_pprev = 0.0;
            self.v_sprev = 0.0;
            self.sec_in_dc = 0.0;
            self.v_pred_prev = [0.0; NUM_NODES];
            self.predict_primed = false;
            self.in_audio_pass = false;
            self.clear_driver_state();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauss_solve_3x3() {
        // [2 1 -1; -3 -1 2; -2 1 2] x = [8; -11; -3]  ->  x = [2, 3, -1]
        let n = 3;
        let m = [[2.0, 1.0, -1.0], [-3.0, -1.0, 2.0], [-2.0, 1.0, 2.0]];
        let mut a = [0.0f64; NUM_NODES * NUM_NODES];
        for (i, row) in m.iter().enumerate() {
            for (j, &val) in row.iter().enumerate() {
                a[i * n + j] = val;
            }
        }
        let mut b = [0.0f64; NUM_NODES];
        b[0] = 8.0;
        b[1] = -11.0;
        b[2] = -3.0;
        assert!(WdfModel::gauss_solve(&mut a, &mut b, n));
        assert!((b[0] - 2.0).abs() < 1e-12, "x0={}", b[0]);
        assert!((b[1] - 3.0).abs() < 1e-12, "x1={}", b[1]);
        assert!((b[2] + 1.0).abs() < 1e-12, "x2={}", b[2]);
    }
}
