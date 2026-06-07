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

// Worst-case fixed-array bounds (RB off uses 17 resistors / 7 caps; sized for RB-on headroom).
const MAX_RES: usize = 24;
const MAX_CAPS: usize = 8;

/// Coupled-circuit MNA + Newton solver. All scratch is fixed-size (no `Vec` on the process path)
/// to satisfy the zero-alloc audio-thread contract.
pub struct WdfModel {
    pub v: [f64; NUM_NODES],      // current node voltages
    pub v_prev: [f64; NUM_NODES], // previous-sample node voltages (cap history)
    known: [bool; NUM_NODES],
    urow: [i32; NUM_NODES], // node -> unknown-row index, or -1
    u: usize,               // number of unknowns

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
    /// (R_KJ ohms), and `bias` (RV1 pot [0,1]). Hardcodes the realtime tier (RB off, U=16).
    /// Mirrors `BA283::build` (`main.cpp:314-409`).
    pub fn build(internal_sr: f64, rgain: f64, bias: f64) -> Self {
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
            known,
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
            newton_tol: RT_NEWTON_TOL,
            max_iters: RT_MAX_ITERS,
            cap_tier: CapTier::Full,
            hist: [0.0; NUM_NODES],
            f: [0.0; NUM_NODES],
            delta: [0.0; NUM_NODES],
            rn: [0.0; NUM_NODES],
            j: [0.0; NUM_NODES * NUM_NODES],
            g0: [0.0; NUM_NODES * NUM_NODES],
        };
        model.build_g0();
        model
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
                let (qbe_dep, _) = depletion_charge(vbe, t.p.cje, t.p.vje, t.p.mje);
                let (qbc_dep, _) = depletion_charge(vbc, t.p.cjc, t.p.vjc, t.p.mjc);
                let ibe = k2 * (qbe_dep + tf * o.itf - t.qbe_prev) - t.ibe_prev;
                let ibc = k2 * (qbc_dep + tr * o.itr - t.qbc_prev) - t.ibc_prev;
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

    /// One Newton solve (up to `max_iters`) at the given `gmin` regularization. Returns iters used.
    /// Realtime path only (`lsTrim=false`): the 3 trim regions (`main.cpp:634-651/665-679/702-712`)
    /// are skipped; this is the eager backtracking-line-search path (`main.cpp:520-714`).
    fn newton_iterate(&mut self, gmin: f64) -> usize {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
        let mut iters_used = 0;
        for iter in 0..self.max_iters {
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
                    let (qbe_dep, cbe_dep) = depletion_charge(vbe, t.p.cje, t.p.vje, t.p.mje);
                    let (qbc_dep, cbc_dep) = depletion_charge(vbc, t.p.cjc, t.p.vjc, t.p.mjc);
                    let qbe = qbe_dep + tf * o.itf;
                    let cbe = cbe_dep + tf * o.gtf;
                    let qbc = qbc_dep + tr * o.itr;
                    let cbc = cbc_dep + tr * o.gtr;
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

            // 2-norm of residual; Newton step is a descent direction for this.
            let mut norm_f = 0.0;
            for i in 0..u {
                norm_f += self.f[i] * self.f[i];
            }
            norm_f = norm_f.sqrt();
            if norm_f < self.newton_tol {
                break;
            }

            for i in 0..u {
                self.delta[i] = -self.f[i];
            }
            if !Self::gauss_solve(&mut self.j, &mut self.delta, u) {
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
        }
        iters_used
    }

    /// Solve one sample. `v[N_A]`, `v[N_INPUT]` must be set before calling (`main.cpp:452-516`).
    pub fn process(&mut self) {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
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
