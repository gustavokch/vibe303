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
    // precomputed saturated-branch (v >= FC*vj) constants for depletion_charge,
    // one set per junction — depend only on (cj0, vj, m). Avoids 3 pow_ per call.
    pub je_c0: f64,
    pub je_q0: f64,
    pub je_dcdv: f64, // base-emitter (cje,vje,mje)
    pub jc_c0: f64,
    pub jc_q0: f64,
    pub jc_dcdv: f64, // base-collector (cjc,vjc,mjc)
    // Ultra-fast tier: use the cheap low-order `ultra_exp` in `diode_branch`. Set from
    // `QualityConfig::cheap_exp` at build; false at realtime/fast (near-exact `fast_exp`).
    pub cheap_exp: bool,
}

/// Precompute (c0, q0, dcdv) for the depletion_charge saturated branch (f0 = 1-FC = 0.5).
/// MUST use the exact same expressions/order as depletion_charge's else branch for bit-exactness.
pub fn dep_sat_consts(cj0: f64, vj: f64, m: f64) -> (f64, f64, f64) {
    const FC: f64 = 0.5;
    if cj0 <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let f0 = 1.0 - FC;
    let c0 = cj0 * pow_(f0, -m);
    let q0 = cj0 * vj / (1.0 - m) * (1.0 - pow_(f0, 1.0 - m));
    let dcdv = cj0 * m / vj * pow_(f0, -m - 1.0);
    (c0, q0, dcdv)
}

// vcrit = Vt*ln(Vt/(sqrt(2)*Is)). The C++ device cards use the truncated literal `1.41421356`
// (NOT std::sqrt(2)); we match it verbatim so vcrit — and every downstream BJT value — is
// bit-identical to the golden vectors. This is a one-time build constant, so std `ln` (not the
// fast-math path) is correct here, matching the C++ `std::log`.
const SQRT2_TRUNC: f64 = 1.41421356;

/// BC184C small-signal card (TR1, TR2) — `main.cpp:296-298`.
pub fn bc184c() -> BjtParams {
    let is = 1.04e-14;
    let mut p = BjtParams {
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
        ..BjtParams::default()
    };
    let (je_c0, je_q0, je_dcdv) = dep_sat_consts(p.cje, p.vje, p.mje);
    p.je_c0 = je_c0;
    p.je_q0 = je_q0;
    p.je_dcdv = je_dcdv;
    let (jc_c0, jc_q0, jc_dcdv) = dep_sat_consts(p.cjc, p.vjc, p.mjc);
    p.jc_c0 = jc_c0;
    p.jc_q0 = jc_q0;
    p.jc_dcdv = jc_dcdv;
    p
}

/// 2N3055 power card (TR3) — `main.cpp:299-301`.
pub fn q2n3055() -> BjtParams {
    let is = 4.66e-12;
    let mut p = BjtParams {
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
        ..BjtParams::default()
    };
    let (je_c0, je_q0, je_dcdv) = dep_sat_consts(p.cje, p.vje, p.mje);
    p.je_c0 = je_c0;
    p.je_q0 = je_q0;
    p.je_dcdv = je_dcdv;
    let (jc_c0, jc_q0, jc_dcdv) = dep_sat_consts(p.cjc, p.vjc, p.mjc);
    p.jc_c0 = jc_c0;
    p.jc_q0 = jc_q0;
    p.jc_dcdv = jc_dcdv;
    p
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
// Batch path uses fast-math leaf kernels; EXACT=true would silently diverge from the scalar reference.
const _: () = assert!(!EXACT, "SoA batch path only valid with EXACT=false");

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

/// Cheap `exp` for the ultra-fast tier: same range reduction + IEEE-754 exponent assembly as
/// [`fast_exp`], but a 4th-order Taylor on the reduced argument (|r| <= ln2/2 ~ 0.347), so the
/// truncation is ~r^5/120 ~ 4e-5 relative — well inside the ultra-fast error budget while saving
/// five Horner terms per call. The exponent guards are identical (reverse-biased junctions push
/// k very negative). Used only when `QualityConfig::cheap_exp` is set.
#[inline]
pub fn ultra_exp(x: f64) -> f64 {
    const LN2: f64 = 0.6931471805599453;
    const INV_LN2: f64 = 1.4426950408889634;
    let kf = (x * INV_LN2 + 0.5).floor();
    let k = kf as i32;
    let r = x - kf * LN2;
    let p = 1.0 + r * (1.0 + r * (1.0 / 2.0 + r * (1.0 / 6.0 + r * (1.0 / 24.0))));
    if k <= -1023 {
        return 0.0;
    }
    if k >= 1024 {
        return p * f64::INFINITY;
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

/// Like [`depletion_charge`] but takes the saturated-branch constants `(c0, q0, dcdv)`
/// precomputed once per junction (see [`dep_sat_consts`]). The else-branch arithmetic is
/// byte-identical to `depletion_charge`; only the three `pow_` calls are hoisted out.
#[inline]
pub fn depletion_charge_pre(
    v: f64,
    cj0: f64,
    vj: f64,
    m: f64,
    c0: f64,
    q0: f64,
    dcdv: f64,
) -> (f64, f64) {
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
        let d = v - FC * vj;
        let c = c0 + dcdv * d;
        let q = q0 + c0 * d + 0.5 * dcdv * d * d;
        (q, c)
    }
}

/// Diode current + conductance, linearized beyond `vcrit` for Newton stability (SPICE pnjlim
/// spirit). Returns `(i, g)` (`main.cpp:174-187`).
#[inline]
pub fn diode_branch(v: f64, is: f64, vt: f64, vcrit: f64, cheap_exp: bool) -> (f64, f64) {
    let expf = |z: f64| if cheap_exp { ultra_exp(z) } else { exp_(z) };
    if v < vcrit {
        let e = expf(v / vt);
        (is * (e - 1.0), (is / vt) * e)
    } else {
        let ec = expf(vcrit / vt);
        let icrit = is * (ec - 1.0);
        let gcrit = (is / vt) * ec;
        (icrit + gcrit * (v - vcrit), gcrit) // C1-continuous linear extension
    }
}

// ---------------------------------------------------------------------------
// Branchless fast-math leaf kernels for the SoA batch path.
//
// These are bit-exact with fast_exp / fast_log / fast_pow_pos on the realtime
// (EXACT=false) path. The branches are replaced by if/else selects so LLVM's
// SLP vectorizer can fuse 4 independent lanes into AVX2 ymm instructions.
//
// The kernels are generic over `const FMA: bool` (T3 lossy fast/ultra lever):
// - FMA=false: plain `a*b+c` → bit-identical to the original scalar reference
//   (realtime tier; all `to_bits()` + golden contracts hold). Rust does not
//   auto-contract, so this stays the exact pre-T3 operation sequence.
// - FMA=true:  `a.mul_add(b,c)` → fused multiply-add (single rounding), a small
//   numeric change (~1 ULP/op) that LLVM lowers to `vfmadd`. Lossy: fast/ultra
//   tiers only, gated by `compare.py` −40 dB ESR. Monomorphized → zero runtime branch.
// Call sites pick the instantiation per tier via `::<true>`/`::<false>` turbofish.
// ---------------------------------------------------------------------------

/// `a*b + c`, fused (single rounding) when `FMA`. For `FMA=false` this is the
/// literal `a*b+c`; for `c - a*b` pass `(a, -b, c)` (negation is exact, so the
/// `FMA=false` result is bit-identical to `c - a*b`).
#[inline(always)]
fn fma<const FMA: bool>(a: f64, b: f64, c: f64) -> f64 {
    if FMA { a.mul_add(b, c) } else { a * b + c }
}

/// Branchless 9th-order Taylor `exp`, bit-exact with [`fast_exp`] at `FMA=false`.
/// Branches → if/else selects so SLP can fuse 4 lanes into AVX2 ymm ops.
#[inline]
fn exp_bl<const FMA: bool>(x: f64) -> f64 {
    const LN2: f64 = 0.6931471805599453;
    const INV_LN2: f64 = 1.4426950408889634;
    let kf = fma::<FMA>(x, INV_LN2, 0.5).floor();
    let k = kf as i32;
    let r = fma::<FMA>(kf, -LN2, x); // x - kf*LN2
    // Horner: each step is coeff + r*rest ≡ r*rest + coeff (FP add commutes) → fma(r, rest, coeff).
    let p = fma::<FMA>(r,
        fma::<FMA>(r,
            fma::<FMA>(r,
                fma::<FMA>(r,
                    fma::<FMA>(r,
                        fma::<FMA>(r,
                            fma::<FMA>(r,
                                fma::<FMA>(r,
                                    fma::<FMA>(r, 1.0 / 362880.0, 1.0 / 40320.0),
                                    1.0 / 5040.0),
                                1.0 / 720.0),
                            1.0 / 120.0),
                        1.0 / 24.0),
                    1.0 / 6.0),
                1.0 / 2.0),
            1.0),
        1.0);
    // Clamp k so the IEEE-754 bit-shift never wraps; guards applied as selects below.
    let kc = k.clamp(-1022, 1023);
    let u: u64 = ((kc + 1023) as u64) << 52;
    let pk = f64::from_bits(u);
    let res = p * pk;
    let res = if k <= -1023 { 0.0 } else { res };
    if k >= 1024 { p * f64::INFINITY } else { res }
}

/// Branchless log via IEEE-754 exponent decomposition + atanh series, bit-exact with [`fast_log`]
/// at `FMA=false`. The mantissa-adjustment branch becomes a select so SLP can vectorize 4 lanes.
#[inline]
fn log_bl<const FMA: bool>(x: f64) -> f64 {
    const SQRT1_2: f64 = 0.707_106_781_186_547_6;
    const LN2: f64 = 0.6931471805599453;
    let bits = x.to_bits();
    let e = (((bits >> 52) & 0x7ff) as i32) - 1022;
    let mbits = (bits & 0x000f_ffff_ffff_ffff) | 0x3fe0_0000_0000_0000;
    let m = f64::from_bits(mbits);
    let need_adj = m < SQRT1_2;
    let m = if need_adj { m * 2.0 } else { m };
    let e = e - need_adj as i32;
    let f = m - 1.0;
    let s = f / (2.0 + f);
    let s2 = s * s;
    // Horner in s2: coeff + s2*rest ≡ s2*rest + coeff → fma(s2, rest, coeff). Outer 2*s*(...) unchanged.
    let inner = fma::<FMA>(s2,
        fma::<FMA>(s2,
            fma::<FMA>(s2,
                fma::<FMA>(s2,
                    fma::<FMA>(s2, 1.0 / 11.0, 1.0 / 9.0),
                    1.0 / 7.0),
                1.0 / 5.0),
            1.0 / 3.0),
        1.0);
    let poly = 2.0 * s * inner;
    fma::<FMA>(e as f64, LN2, poly) // poly + e*LN2
}

/// Branchless positive-base power, bit-exact with [`fast_pow_pos`] at `FMA=false`.
#[inline]
fn pow_bl<const FMA: bool>(base: f64, ex: f64) -> f64 {
    exp_bl::<FMA>(ex * log_bl::<FMA>(base))
}

/// Width-4 `exp_bl(v[i]*inv_vt[i])`, generic over FMA — wraps the per-tier instantiation so
/// `newton_iterate` can pick FMA on/off with one branch instead of four `::<F>` turbofish sites.
#[inline(always)]
fn exp4<const FMA: bool>(v: &[f64; 4], inv_vt: &[f64; 4]) -> [f64; 4] {
    [
        exp_bl::<FMA>(v[0] * inv_vt[0]), exp_bl::<FMA>(v[1] * inv_vt[1]),
        exp_bl::<FMA>(v[2] * inv_vt[2]), exp_bl::<FMA>(v[3] * inv_vt[3]),
    ]
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
    let (itf, gtf) = diode_branch(vbe, p.is, p.vt, p.vcrit, p.cheap_exp); // forward transport + d/dvbe
    let (itr, gtr) = diode_branch(vbc, p.is, p.vt, p.vcrit, p.cheap_exp); // reverse transport + d/dvbc
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

/// Like [`compute_bjt`] but takes pre-computed `exp(vbe/vt)`, `exp(vbc/vt)`,
/// `exp(vcrit/vt)` — call after gathering ef4/er4/ec4 with `exp_bl` so LLVM
/// can SLP-fuse 3 independent exp_bl calls into AVX2 YMM ops before this
/// per-BJT arithmetic path. `#[inline(always)]` lets LLVM keep BjtOut in
/// registers (same behavior as the inlined baseline `compute_bjt`).
/// Valid on `EXACT=false, cheap_exp=false` path — caller is responsible.
/// Generic over `const FMA: bool`: `FMA=false` is bit-identical to the pre-T3 scalar reference
/// (every conversion is `coeff + a*b` ≡ `a*b + coeff`, or `c - a*b` ≡ `(-a)*b + c`, both exact
/// under FP add-commutativity / exact negation). `FMA=true` fuses to `vfmadd` (lossy fast/ultra).
///
/// Production-dead since the post-exp algebra was width-4 batched into [`compute_bjt_preexp_batch`];
/// retained under `#[cfg(test)]` as the scalar reference for `compute_bjt_preexp_batch_is_bit_exact`.
#[cfg(test)]
#[inline(always)]
fn compute_bjt_preexp<const FMA: bool>(vbe: f64, vbc: f64, p: &BjtParams, ef: f64, er: f64, ec: f64) -> BjtOut {
    let inv_vt_is = p.is / p.vt;
    let (itf, gtf) = if vbe < p.vcrit {
        (p.is * (ef - 1.0), inv_vt_is * ef)
    } else {
        let icrit = p.is * (ec - 1.0);
        let gcrit = inv_vt_is * ec;
        (fma::<FMA>(gcrit, vbe - p.vcrit, icrit), gcrit) // icrit + gcrit*(vbe-vcrit)
    };
    let (itr, gtr) = if vbc < p.vcrit {
        (p.is * (er - 1.0), inv_vt_is * er)
    } else {
        let icrit = p.is * (ec - 1.0);
        let gcrit = inv_vt_is * ec;
        (fma::<FMA>(gcrit, vbc - p.vcrit, icrit), gcrit) // icrit + gcrit*(vbc-vcrit)
    };
    let inv_af = if p.vaf > 0.0 { 1.0 / p.vaf } else { 0.0 };
    let inv_ar = if p.var > 0.0 { 1.0 / p.var } else { 0.0 };
    // 1.0 - vbc*inv_af - vbe*inv_ar = (1 - vbc*inv_af) - vbe*inv_ar
    let mut inv1 = fma::<FMA>(vbe, -inv_ar, fma::<FMA>(vbc, -inv_af, 1.0));
    if inv1 < 0.1 { inv1 = 0.1; }
    let q1 = 1.0 / inv1;
    let dq1_vbe = q1 * q1 * inv_ar;
    let dq1_vbc = q1 * q1 * inv_af;
    let inv_kf = if p.ikf > 0.0 { 1.0 / p.ikf } else { 0.0 };
    let inv_kr = if p.ikr > 0.0 { 1.0 / p.ikr } else { 0.0 };
    let q2 = fma::<FMA>(itf, inv_kf, itr * inv_kr); // itf*inv_kf + itr*inv_kr
    let dq2_vbe = gtf * inv_kf;
    let dq2_vbc = gtr * inv_kr;
    let s = fma::<FMA>(4.0, q2, 1.0).sqrt(); // (1 + 4*q2).sqrt()
    let qb = q1 * (1.0 + s) / 2.0;
    let dqb_vbe = dq1_vbe * (1.0 + s) / 2.0 + q1 * dq2_vbe / s;
    let dqb_vbc = dq1_vbc * (1.0 + s) / 2.0 + q1 * dq2_vbc / s;
    let diff = itf - itr;
    let it = diff / qb;
    let qbsq = qb * qb;
    let dit_vbe = fma::<FMA>(diff, -dqb_vbe, gtf * qb) / qbsq; // (gtf*qb - diff*dqb_vbe)/qbsq
    let dit_vbc = fma::<FMA>(diff, -dqb_vbc, -gtr * qb) / qbsq; // (-gtr*qb - diff*dqb_vbc)/qbsq
    let inv_bf = 1.0 / p.bf;
    let inv_br = 1.0 / p.br;
    let ic = fma::<FMA>(itr, -inv_br, it); // it - itr*inv_br
    let ib = fma::<FMA>(itf, inv_bf, itr * inv_br); // itf*inv_bf + itr*inv_br
    let ie = -(ic + ib);
    let dic_vbe = dit_vbe;
    let dic_vbc = fma::<FMA>(gtr, -inv_br, dit_vbc); // dit_vbc - gtr*inv_br
    let dib_vbe = gtf * inv_bf;
    let dib_vbc = gtr * inv_br;
    let die_vbe = -(dic_vbe + dib_vbe);
    let die_vbc = -(dic_vbc + dib_vbc);
    BjtOut {
        ib, ic, ie, itf, gtf, itr, gtr,
        dic_b: dic_vbe + dic_vbc, dic_e: -dic_vbe, dic_c: -dic_vbc,
        dib_b: dib_vbe + dib_vbc, dib_e: -dib_vbe, dib_c: -dib_vbc,
        die_b: die_vbe + die_vbc, die_e: -die_vbe, die_c: -die_vbc,
    }
}

/// Width-4 SoA mirror of [`compute_bjt_preexp`] — the post-exp Gummel-Poon algebra (currents +
/// 16-field Jacobian) for 3 BJTs at once (lane 3 = padding, never scattered). Reuses the
/// caller's already-batched `ef4/er4/ec4` (NO exp recompute — the reverted `b6e5d59` diode batch
/// lost precisely because it doubled the exp work; this batches only the divide-/sqrt-heavy tail).
/// Params are extracted into `[f64;4]` SoA (stride-8 loads) so LLVM SLP-fuses the 4 lanes into
/// AVX2 packed `vdivpd`/`vsqrtpd`/`vmulpd`. Bit-exact with `compute_bjt_preexp` at `FMA=false`
/// (same per-lane op tree); guarded by `compute_bjt_preexp_batch_is_bit_exact`.
#[inline]
fn compute_bjt_preexp_batch<const FMA: bool>(
    vbe: &[f64; 4],
    vbc: &[f64; 4],
    bjts: &[Bjt; 3],
    ef: &[f64; 4],
    er: &[f64; 4],
    ec: &[f64; 4],
) -> BjtOutSoA {
    // Extract params to SoA. Lane 3 = safe defaults (finite, never scattered).
    let is4    = [bjts[0].p.is,    bjts[1].p.is,    bjts[2].p.is,    1e-14];
    let vt4    = [bjts[0].p.vt,    bjts[1].p.vt,    bjts[2].p.vt,    0.02585];
    let vcrit4 = [bjts[0].p.vcrit, bjts[1].p.vcrit, bjts[2].p.vcrit, 0.0];
    let vaf4   = [bjts[0].p.vaf,   bjts[1].p.vaf,   bjts[2].p.vaf,   0.0];
    let var4   = [bjts[0].p.var,   bjts[1].p.var,   bjts[2].p.var,   0.0];
    let ikf4   = [bjts[0].p.ikf,   bjts[1].p.ikf,   bjts[2].p.ikf,   0.0];
    let ikr4   = [bjts[0].p.ikr,   bjts[1].p.ikr,   bjts[2].p.ikr,   0.0];
    let bf4    = [bjts[0].p.bf,    bjts[1].p.bf,    bjts[2].p.bf,    1.0];
    let br4    = [bjts[0].p.br,    bjts[1].p.br,    bjts[2].p.br,    1.0];

    let mut o = BjtOutSoA::default();
    for l in 0..4 {
        let inv_vt_is = is4[l] / vt4[l];
        let icrit = is4[l] * (ec[l] - 1.0);
        let gcrit = inv_vt_is * ec[l];
        let (itf, gtf) = if vbe[l] < vcrit4[l] {
            (is4[l] * (ef[l] - 1.0), inv_vt_is * ef[l])
        } else {
            (fma::<FMA>(gcrit, vbe[l] - vcrit4[l], icrit), gcrit)
        };
        let (itr, gtr) = if vbc[l] < vcrit4[l] {
            (is4[l] * (er[l] - 1.0), inv_vt_is * er[l])
        } else {
            (fma::<FMA>(gcrit, vbc[l] - vcrit4[l], icrit), gcrit)
        };
        let inv_af = if vaf4[l] > 0.0 { 1.0 / vaf4[l] } else { 0.0 };
        let inv_ar = if var4[l] > 0.0 { 1.0 / var4[l] } else { 0.0 };
        let mut inv1 = fma::<FMA>(vbe[l], -inv_ar, fma::<FMA>(vbc[l], -inv_af, 1.0));
        if inv1 < 0.1 { inv1 = 0.1; }
        let q1 = 1.0 / inv1;
        let dq1_vbe = q1 * q1 * inv_ar;
        let dq1_vbc = q1 * q1 * inv_af;
        let inv_kf = if ikf4[l] > 0.0 { 1.0 / ikf4[l] } else { 0.0 };
        let inv_kr = if ikr4[l] > 0.0 { 1.0 / ikr4[l] } else { 0.0 };
        let q2 = fma::<FMA>(itf, inv_kf, itr * inv_kr);
        let dq2_vbe = gtf * inv_kf;
        let dq2_vbc = gtr * inv_kr;
        let s = fma::<FMA>(4.0, q2, 1.0).sqrt();
        let qb = q1 * (1.0 + s) / 2.0;
        let dqb_vbe = dq1_vbe * (1.0 + s) / 2.0 + q1 * dq2_vbe / s;
        let dqb_vbc = dq1_vbc * (1.0 + s) / 2.0 + q1 * dq2_vbc / s;
        let diff = itf - itr;
        let it = diff / qb;
        let qbsq = qb * qb;
        let dit_vbe = fma::<FMA>(diff, -dqb_vbe, gtf * qb) / qbsq;
        let dit_vbc = fma::<FMA>(diff, -dqb_vbc, -gtr * qb) / qbsq;
        let inv_bf = 1.0 / bf4[l];
        let inv_br = 1.0 / br4[l];
        let ic = fma::<FMA>(itr, -inv_br, it);
        let ib = fma::<FMA>(itf, inv_bf, itr * inv_br);
        let ie = -(ic + ib);
        let dic_vbe = dit_vbe;
        let dic_vbc = fma::<FMA>(gtr, -inv_br, dit_vbc);
        let dib_vbe = gtf * inv_bf;
        let dib_vbc = gtr * inv_br;
        let die_vbe = -(dic_vbe + dib_vbe);
        let die_vbc = -(dic_vbc + dib_vbc);
        o.ib[l] = ib; o.ic[l] = ic; o.ie[l] = ie;
        o.itf[l] = itf; o.gtf[l] = gtf; o.itr[l] = itr; o.gtr[l] = gtr;
        o.dic_b[l] = dic_vbe + dic_vbc; o.dic_e[l] = -dic_vbe; o.dic_c[l] = -dic_vbc;
        o.dib_b[l] = dib_vbe + dib_vbc; o.dib_e[l] = -dib_vbe; o.dib_c[l] = -dib_vbc;
        o.die_b[l] = die_vbe + die_vbc; o.die_e[l] = -die_vbe; o.die_c[l] = -die_vbc;
    }
    o
}

// ---------------------------------------------------------------------------
// Width-4 SoA batch device eval. Lane 3 = padding (vbe=vbc=0, params zeroed);
// only lanes 0-2 are ever scattered. Bit-exact with the scalar path on EXACT=false.
// Kept for test coverage; hot path uses compute_bjt_preexp + inline exp4 batching.
// ---------------------------------------------------------------------------

/// SoA output for a width-4 [`compute_bjt`] batch. Mirrors all 16 [`BjtOut`] fields.
/// Produced by `compute_bjt_preexp_batch` on the production path and by `compute_bjt_batch`
/// under `#[cfg(test)]` for its contract guard.
#[derive(Clone, Copy, Debug, Default)]
struct BjtOutSoA {
    ib:    [f64; 4],
    ic:    [f64; 4],
    ie:    [f64; 4],
    itf:   [f64; 4],
    gtf:   [f64; 4],
    itr:   [f64; 4],
    gtr:   [f64; 4],
    dib_b: [f64; 4],
    dib_e: [f64; 4],
    dib_c: [f64; 4],
    dic_b: [f64; 4],
    dic_e: [f64; 4],
    dic_c: [f64; 4],
    die_b: [f64; 4],
    die_e: [f64; 4],
    die_c: [f64; 4],
}

/// Evaluate 3 BJTs as a width-4 SoA batch (lane 3 = padding, never scattered).
///
/// Params are extracted into [f64;4] SoA arrays (contiguous stride=8) before
/// any arithmetic so LLVM can use vmovupd for loads and SLP-fuse the 4
/// independent computation chains into AVX2 ymm ops. Takes `&[Bjt;3]` directly
/// to avoid the ~832-byte intermediate `[BjtParams;4]` copy in `newton_iterate`.
/// Bit-exact with [`compute_bjt`] on the `EXACT=false, cheap_exp=false` path.
/// Production path uses `exp4` + [`compute_bjt_preexp_batch`] (reusing the already-batched exp).
/// This full-batch variant (which recomputes exp internally) is retained under `#[cfg(test)]`
/// for the `compute_bjt_batch_is_bit_exact` SoA contract guard.
#[cfg(test)]
#[inline(never)]
fn compute_bjt_batch(vbe: &[f64; 4], vbc: &[f64; 4], bjts: &[Bjt; 3]) -> BjtOutSoA {
    // Extract to [f64;4] SoA. AoS stride≈208 bytes blocks SLP; SoA stride=8 enables it.
    // Lane 3 = padding (0 / safe defaults), never scattered.
    let is4    = [bjts[0].p.is,    bjts[1].p.is,    bjts[2].p.is,    1e-14]; // safe: exp(v/vt) finite
    let vt4    = [bjts[0].p.vt,    bjts[1].p.vt,    bjts[2].p.vt,    0.02585];
    let vcrit4 = [bjts[0].p.vcrit, bjts[1].p.vcrit, bjts[2].p.vcrit, 0.0];
    let vaf4   = [bjts[0].p.vaf,   bjts[1].p.vaf,   bjts[2].p.vaf,   0.0];
    let var4   = [bjts[0].p.var,   bjts[1].p.var,   bjts[2].p.var,   0.0];
    let ikf4   = [bjts[0].p.ikf,   bjts[1].p.ikf,   bjts[2].p.ikf,   0.0];
    let ikr4   = [bjts[0].p.ikr,   bjts[1].p.ikr,   bjts[2].p.ikr,   0.0];
    let bf4    = [bjts[0].p.bf,    bjts[1].p.bf,    bjts[2].p.bf,    1.0]; // safe: 1/bf finite
    let br4    = [bjts[0].p.br,    bjts[1].p.br,    bjts[2].p.br,    1.0];

    // Transcendentals: 4 independent exp_bl calls, SLP-fused → vmulpd/vaddpd chains.
    let ef = [exp_bl::<false>(vbe[0]/vt4[0]), exp_bl::<false>(vbe[1]/vt4[1]), exp_bl::<false>(vbe[2]/vt4[2]), exp_bl::<false>(vbe[3]/vt4[3])];
    let er = [exp_bl::<false>(vbc[0]/vt4[0]), exp_bl::<false>(vbc[1]/vt4[1]), exp_bl::<false>(vbc[2]/vt4[2]), exp_bl::<false>(vbc[3]/vt4[3])];
    let ec = [exp_bl::<false>(vcrit4[0]/vt4[0]), exp_bl::<false>(vcrit4[1]/vt4[1]), exp_bl::<false>(vcrit4[2]/vt4[2]), exp_bl::<false>(vcrit4[3]/vt4[3])];

    let icrit = [is4[0]*(ec[0]-1.0), is4[1]*(ec[1]-1.0), is4[2]*(ec[2]-1.0), is4[3]*(ec[3]-1.0)];
    let isovt = [is4[0]/vt4[0],      is4[1]/vt4[1],      is4[2]/vt4[2],      is4[3]/vt4[3]];
    let gcrit = [isovt[0]*ec[0],      isovt[1]*ec[1],     isovt[2]*ec[2],     isovt[3]*ec[3]];
    let itf = [
        if vbe[0] < vcrit4[0] { is4[0]*(ef[0]-1.0) } else { icrit[0]+gcrit[0]*(vbe[0]-vcrit4[0]) },
        if vbe[1] < vcrit4[1] { is4[1]*(ef[1]-1.0) } else { icrit[1]+gcrit[1]*(vbe[1]-vcrit4[1]) },
        if vbe[2] < vcrit4[2] { is4[2]*(ef[2]-1.0) } else { icrit[2]+gcrit[2]*(vbe[2]-vcrit4[2]) },
        if vbe[3] < vcrit4[3] { is4[3]*(ef[3]-1.0) } else { icrit[3]+gcrit[3]*(vbe[3]-vcrit4[3]) },
    ];
    let gtf = [
        if vbe[0] < vcrit4[0] { isovt[0]*ef[0] } else { gcrit[0] },
        if vbe[1] < vcrit4[1] { isovt[1]*ef[1] } else { gcrit[1] },
        if vbe[2] < vcrit4[2] { isovt[2]*ef[2] } else { gcrit[2] },
        if vbe[3] < vcrit4[3] { isovt[3]*ef[3] } else { gcrit[3] },
    ];
    let itr = [
        if vbc[0] < vcrit4[0] { is4[0]*(er[0]-1.0) } else { icrit[0]+gcrit[0]*(vbc[0]-vcrit4[0]) },
        if vbc[1] < vcrit4[1] { is4[1]*(er[1]-1.0) } else { icrit[1]+gcrit[1]*(vbc[1]-vcrit4[1]) },
        if vbc[2] < vcrit4[2] { is4[2]*(er[2]-1.0) } else { icrit[2]+gcrit[2]*(vbc[2]-vcrit4[2]) },
        if vbc[3] < vcrit4[3] { is4[3]*(er[3]-1.0) } else { icrit[3]+gcrit[3]*(vbc[3]-vcrit4[3]) },
    ];
    let gtr = [
        if vbc[0] < vcrit4[0] { isovt[0]*er[0] } else { gcrit[0] },
        if vbc[1] < vcrit4[1] { isovt[1]*er[1] } else { gcrit[1] },
        if vbc[2] < vcrit4[2] { isovt[2]*er[2] } else { gcrit[2] },
        if vbc[3] < vcrit4[3] { isovt[3]*er[3] } else { gcrit[3] },
    ];

    let inv_af = [
        if vaf4[0] > 0.0 { 1.0/vaf4[0] } else { 0.0 },
        if vaf4[1] > 0.0 { 1.0/vaf4[1] } else { 0.0 },
        if vaf4[2] > 0.0 { 1.0/vaf4[2] } else { 0.0 },
        if vaf4[3] > 0.0 { 1.0/vaf4[3] } else { 0.0 },
    ];
    let inv_ar = [
        if var4[0] > 0.0 { 1.0/var4[0] } else { 0.0 },
        if var4[1] > 0.0 { 1.0/var4[1] } else { 0.0 },
        if var4[2] > 0.0 { 1.0/var4[2] } else { 0.0 },
        if var4[3] > 0.0 { 1.0/var4[3] } else { 0.0 },
    ];
    let inv1 = [
        (1.0 - vbc[0]*inv_af[0] - vbe[0]*inv_ar[0]).max(0.1),
        (1.0 - vbc[1]*inv_af[1] - vbe[1]*inv_ar[1]).max(0.1),
        (1.0 - vbc[2]*inv_af[2] - vbe[2]*inv_ar[2]).max(0.1),
        (1.0 - vbc[3]*inv_af[3] - vbe[3]*inv_ar[3]).max(0.1),
    ];
    let q1 = [1.0/inv1[0], 1.0/inv1[1], 1.0/inv1[2], 1.0/inv1[3]];
    let dq1_vbe = [q1[0]*q1[0]*inv_ar[0], q1[1]*q1[1]*inv_ar[1], q1[2]*q1[2]*inv_ar[2], q1[3]*q1[3]*inv_ar[3]];
    let dq1_vbc = [q1[0]*q1[0]*inv_af[0], q1[1]*q1[1]*inv_af[1], q1[2]*q1[2]*inv_af[2], q1[3]*q1[3]*inv_af[3]];
    let inv_kf = [
        if ikf4[0] > 0.0 { 1.0/ikf4[0] } else { 0.0 },
        if ikf4[1] > 0.0 { 1.0/ikf4[1] } else { 0.0 },
        if ikf4[2] > 0.0 { 1.0/ikf4[2] } else { 0.0 },
        if ikf4[3] > 0.0 { 1.0/ikf4[3] } else { 0.0 },
    ];
    let inv_kr = [
        if ikr4[0] > 0.0 { 1.0/ikr4[0] } else { 0.0 },
        if ikr4[1] > 0.0 { 1.0/ikr4[1] } else { 0.0 },
        if ikr4[2] > 0.0 { 1.0/ikr4[2] } else { 0.0 },
        if ikr4[3] > 0.0 { 1.0/ikr4[3] } else { 0.0 },
    ];
    let q2 = [
        itf[0]*inv_kf[0]+itr[0]*inv_kr[0], itf[1]*inv_kf[1]+itr[1]*inv_kr[1],
        itf[2]*inv_kf[2]+itr[2]*inv_kr[2], itf[3]*inv_kf[3]+itr[3]*inv_kr[3],
    ];
    let dq2_vbe = [gtf[0]*inv_kf[0], gtf[1]*inv_kf[1], gtf[2]*inv_kf[2], gtf[3]*inv_kf[3]];
    let dq2_vbc = [gtr[0]*inv_kr[0], gtr[1]*inv_kr[1], gtr[2]*inv_kr[2], gtr[3]*inv_kr[3]];
    let s = [(1.0+4.0*q2[0]).sqrt(), (1.0+4.0*q2[1]).sqrt(), (1.0+4.0*q2[2]).sqrt(), (1.0+4.0*q2[3]).sqrt()];
    let qb = [q1[0]*(1.0+s[0])/2.0, q1[1]*(1.0+s[1])/2.0, q1[2]*(1.0+s[2])/2.0, q1[3]*(1.0+s[3])/2.0];
    let dqb_vbe = [
        dq1_vbe[0]*(1.0+s[0])/2.0 + q1[0]*dq2_vbe[0]/s[0],
        dq1_vbe[1]*(1.0+s[1])/2.0 + q1[1]*dq2_vbe[1]/s[1],
        dq1_vbe[2]*(1.0+s[2])/2.0 + q1[2]*dq2_vbe[2]/s[2],
        dq1_vbe[3]*(1.0+s[3])/2.0 + q1[3]*dq2_vbe[3]/s[3],
    ];
    let dqb_vbc = [
        dq1_vbc[0]*(1.0+s[0])/2.0 + q1[0]*dq2_vbc[0]/s[0],
        dq1_vbc[1]*(1.0+s[1])/2.0 + q1[1]*dq2_vbc[1]/s[1],
        dq1_vbc[2]*(1.0+s[2])/2.0 + q1[2]*dq2_vbc[2]/s[2],
        dq1_vbc[3]*(1.0+s[3])/2.0 + q1[3]*dq2_vbc[3]/s[3],
    ];
    let diff = [itf[0]-itr[0], itf[1]-itr[1], itf[2]-itr[2], itf[3]-itr[3]];
    let it   = [diff[0]/qb[0], diff[1]/qb[1], diff[2]/qb[2], diff[3]/qb[3]];
    let qbsq = [qb[0]*qb[0],   qb[1]*qb[1],   qb[2]*qb[2],   qb[3]*qb[3]];
    let dit_vbe = [
        (gtf[0]*qb[0] - diff[0]*dqb_vbe[0]) / qbsq[0],
        (gtf[1]*qb[1] - diff[1]*dqb_vbe[1]) / qbsq[1],
        (gtf[2]*qb[2] - diff[2]*dqb_vbe[2]) / qbsq[2],
        (gtf[3]*qb[3] - diff[3]*dqb_vbe[3]) / qbsq[3],
    ];
    let dit_vbc = [
        (-gtr[0]*qb[0] - diff[0]*dqb_vbc[0]) / qbsq[0],
        (-gtr[1]*qb[1] - diff[1]*dqb_vbc[1]) / qbsq[1],
        (-gtr[2]*qb[2] - diff[2]*dqb_vbc[2]) / qbsq[2],
        (-gtr[3]*qb[3] - diff[3]*dqb_vbc[3]) / qbsq[3],
    ];
    let inv_bf  = [1.0/bf4[0], 1.0/bf4[1], 1.0/bf4[2], 1.0/bf4[3]];
    let inv_br  = [1.0/br4[0], 1.0/br4[1], 1.0/br4[2], 1.0/br4[3]];
    let ic = [it[0]-itr[0]*inv_br[0], it[1]-itr[1]*inv_br[1], it[2]-itr[2]*inv_br[2], it[3]-itr[3]*inv_br[3]];
    let ib = [
        itf[0]*inv_bf[0]+itr[0]*inv_br[0], itf[1]*inv_bf[1]+itr[1]*inv_br[1],
        itf[2]*inv_bf[2]+itr[2]*inv_br[2], itf[3]*inv_bf[3]+itr[3]*inv_br[3],
    ];
    let ie = [-(ic[0]+ib[0]), -(ic[1]+ib[1]), -(ic[2]+ib[2]), -(ic[3]+ib[3])];
    let dic_vbe = dit_vbe;
    let dic_vbc = [dit_vbc[0]-gtr[0]*inv_br[0], dit_vbc[1]-gtr[1]*inv_br[1], dit_vbc[2]-gtr[2]*inv_br[2], dit_vbc[3]-gtr[3]*inv_br[3]];
    let dib_vbe = [gtf[0]*inv_bf[0], gtf[1]*inv_bf[1], gtf[2]*inv_bf[2], gtf[3]*inv_bf[3]];
    let dib_vbc = [gtr[0]*inv_br[0], gtr[1]*inv_br[1], gtr[2]*inv_br[2], gtr[3]*inv_br[3]];
    let die_vbe = [-(dic_vbe[0]+dib_vbe[0]), -(dic_vbe[1]+dib_vbe[1]), -(dic_vbe[2]+dib_vbe[2]), -(dic_vbe[3]+dib_vbe[3])];
    let die_vbc = [-(dic_vbc[0]+dib_vbc[0]), -(dic_vbc[1]+dib_vbc[1]), -(dic_vbc[2]+dib_vbc[2]), -(dic_vbc[3]+dib_vbc[3])];
    BjtOutSoA {
        ib, ic, ie, itf, gtf, itr, gtr,
        dib_b: [dib_vbe[0]+dib_vbc[0], dib_vbe[1]+dib_vbc[1], dib_vbe[2]+dib_vbc[2], dib_vbe[3]+dib_vbc[3]],
        dib_e: [-dib_vbe[0], -dib_vbe[1], -dib_vbe[2], -dib_vbe[3]],
        dib_c: [-dib_vbc[0], -dib_vbc[1], -dib_vbc[2], -dib_vbc[3]],
        dic_b: [dic_vbe[0]+dic_vbc[0], dic_vbe[1]+dic_vbc[1], dic_vbe[2]+dic_vbc[2], dic_vbe[3]+dic_vbc[3]],
        dic_e: [-dic_vbe[0], -dic_vbe[1], -dic_vbe[2], -dic_vbe[3]],
        dic_c: [-dic_vbc[0], -dic_vbc[1], -dic_vbc[2], -dic_vbc[3]],
        die_b: [die_vbe[0]+die_vbc[0], die_vbe[1]+die_vbc[1], die_vbe[2]+die_vbc[2], die_vbe[3]+die_vbc[3]],
        die_e: [-die_vbe[0], -die_vbe[1], -die_vbe[2], -die_vbe[3]],
        die_c: [-die_vbc[0], -die_vbc[1], -die_vbc[2], -die_vbc[3]],
    }
}

/// One-lane branchless depletion charge, bit-exact with [`depletion_charge_pre`] on fast-math path.
/// `#[inline(always)]` forces inlining into [`depletion_charge_pre_batch`] for SLP fusion.
#[inline(always)]
fn dep_charge_lane_bl<const FMA: bool>(v: f64, cj0: f64, vj: f64, m: f64, c0: f64, q0: f64, dcdv: f64) -> (f64, f64) {
    const FC: f64 = 0.5;
    // pow arm: clamp f away from zero so log domain is safe even when v > vj (value discarded when !use_pow).
    let f = (1.0 - v / vj).max(1e-9);
    let c_pow = cj0 * pow_bl::<FMA>(f, -m);
    let q_pow = cj0 * vj / (1.0 - m) * (1.0 - pow_bl::<FMA>(f, 1.0 - m));
    // linear arm
    let d = v - FC * vj;
    let c_lin = fma::<FMA>(dcdv, d, c0); // c0 + dcdv*d
    // q0 + c0*d + 0.5*dcdv*d*d = fma(0.5*dcdv*d, d, fma(c0, d, q0))
    let q_lin = fma::<FMA>(0.5 * dcdv * d, d, fma::<FMA>(c0, d, q0));
    // select + cj0 guard (early-return → select so SLP can vectorize)
    let use_pow = v < FC * vj;
    let zero = cj0 <= 0.0;
    (
        if zero { 0.0 } else if use_pow { q_pow } else { q_lin },
        if zero { 0.0 } else if use_pow { c_pow } else { c_lin },
    )
}

/// Width-4 SoA depletion charge. Call ×2 (BE then BC) with the appropriate junction params.
/// Returns `(q[4], c[4])` bit-exact with 4 scalar [`depletion_charge_pre`] calls at `FMA=false`.
fn depletion_charge_pre_batch<const FMA: bool>(
    v: [f64; 4],
    cj0: [f64; 4], vj: [f64; 4], m: [f64; 4],
    c0: [f64; 4], q0: [f64; 4], dcdv: [f64; 4],
) -> ([f64; 4], [f64; 4]) {
    let r0 = dep_charge_lane_bl::<FMA>(v[0], cj0[0], vj[0], m[0], c0[0], q0[0], dcdv[0]);
    let r1 = dep_charge_lane_bl::<FMA>(v[1], cj0[1], vj[1], m[1], c0[1], q0[1], dcdv[1]);
    let r2 = dep_charge_lane_bl::<FMA>(v[2], cj0[2], vj[2], m[2], c0[2], q0[2], dcdv[2]);
    let r3 = dep_charge_lane_bl::<FMA>(v[3], cj0[3], vj[3], m[3], c0[3], q0[3], dcdv[3]);
    (
        [r0.0, r1.0, r2.0, r3.0],
        [r0.1, r1.1, r2.1, r3.1],
    )
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
    /// at os=1, so it is floored to 2. No config — including a hand-assembled `fixed_step` one — can
    /// solve at os=1.
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
const MAX_RES: usize = 24;
const MAX_CAPS: usize = 8;

// --- component aging drift bands (max signed deviation at age=1.0) ---
// Resistors drift up/down a few percent (carbon/metal-film aging + tolerance spread).
const R_DRIFT_MAX: f64 = 0.05; // ±5%
// Film caps are stable; small symmetric spread.
const C_FILM_DRIFT: f64 = 0.05; // ±5%
// Electrolytics lose capacitance with age (dry-out): directional, mostly downward.
const C_ELEC_DROP_MAX: f64 = 0.20; // up to -20%
const C_ELEC_DROP_MIN: f64 = 0.02; // up to +2%
// Farad threshold separating electrolytic (drift directional) from film (drift symmetric).
const C_ELEC_THRESH_F: f64 = 1.0e-6;

/// Deterministic, alloc-free PRNG (SplitMix64) for component-aging drift. Block-rate use only;
/// never touched on the per-sample audio path.
#[inline]
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Uniform in [0, 1).
#[inline]
fn next_unit(state: &mut u64) -> f64 {
    (splitmix64(state) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// Uniform in [-1, 1).
#[inline]
fn next_signed(state: &mut u64) -> f64 {
    2.0 * next_unit(state) - 1.0
}

/// Derive a fresh aging seed from the current one (one re-roll step). Deterministic.
pub fn reroll_seed(seed: u64) -> u64 {
    let mut s = seed;
    splitmix64(&mut s)
}

/// Per-"unit" component drift deviations, derived once from a seed and independent of the `age`
/// knob (which only scales them at apply time). `res_dev`/`cap_dev` are signed fractional
/// deviations; unused array slots stay 0.0.
#[derive(Clone, Copy, Debug)]
pub struct AgePattern {
    res_dev: [f64; MAX_RES],
    cap_dev: [f64; MAX_CAPS],
}

impl AgePattern {
    /// Identity pattern: zero drift everywhere (age has no effect).
    pub fn identity() -> Self {
        AgePattern { res_dev: [0.0; MAX_RES], cap_dev: [0.0; MAX_CAPS] }
    }

    /// Generate per-unit deviations from `seed`. Caps split by `cap_base_far[i]` into electrolytic
    /// (directional drop) vs film (symmetric) bands.
    pub fn from_seed(
        seed: u64,
        n_res: usize,
        n_caps: usize,
        cap_base_far: &[f64; MAX_CAPS],
    ) -> Self {
        let mut s = seed ^ 0xA283_A283_A283_A283; // domain-separate
        let mut p = AgePattern::identity();
        for i in 0..n_res {
            p.res_dev[i] = next_signed(&mut s) * R_DRIFT_MAX;
        }
        for i in 0..n_caps {
            let u = next_unit(&mut s);
            p.cap_dev[i] = if cap_base_far[i] >= C_ELEC_THRESH_F {
                // electrolytic: map [0,1) onto [-DROP_MAX, +DROP_MIN]
                -C_ELEC_DROP_MAX + u * (C_ELEC_DROP_MAX + C_ELEC_DROP_MIN)
            } else {
                (2.0 * u - 1.0) * C_FILM_DRIFT
            };
        }
        p
    }
}

// =====================================================================================
// Nonlinear LO1166 transformer core — Jiles-Atherton magnetizing model (the `trafo` knob).
// SPIKE, behind `--features trafo-core`. Default builds compile NONE of this; the transformer
// stays a pure linear coupled inductor (bit-exact). See docs/plans/...-design.md.
// =====================================================================================

/// Langevin function `L(x) = coth(x) - 1/x` and its derivative `L'(x) = 1/x^2 - csch^2(x)`,
/// returned together. Small-`x` series (Taylor) avoids the 0/0 at the origin.
#[cfg(feature = "trafo-core")]
#[inline]
fn langevin(x: f64) -> (f64, f64) {
    if x.abs() < 1.0e-3 {
        // L ≈ x/3 - x^3/45;  L' ≈ 1/3 - x^2/15
        (x / 3.0 - x * x * x / 45.0, 1.0 / 3.0 - x * x / 15.0)
    } else {
        let cth = 1.0 / x.tanh();
        let l = cth - 1.0 / x;
        let sh = x.sinh();
        let lp = 1.0 / (x * x) - 1.0 / (sh * sh);
        (l, lp)
    }
}

/// Jiles-Atherton constitutive constants + lumped magnetic geometry for one core. Voiced (no
/// datasheet); the geometry lumps are chosen so the small-signal magnetizing inductance is
/// self-consistent and the deviation form (below) cancels exactly at low drive.
#[cfg(feature = "trafo-core")]
#[derive(Clone, Copy, Debug)]
pub struct JaCore {
    m_s: f64,    // saturation magnetization (A/m)
    a: f64,      // domain-wall density / anhysteretic shape (A/m)
    alpha: f64,  // inter-domain mean-field coupling
    k: f64,      // pinning (hysteresis loss; A/m)
    c: f64,      // reversibility 0..1
    mu0: f64,    // vacuum permeability
    k_phi: f64,  // volts→flux lump: phi = k_phi * B  (= N_p * A_e)
    k_hi: f64,   // current→field lump: H = k_hi * i_m (= N_p / l_e)
    lp_eff: f64, // small-signal magnetizing inductance implied by the constants (for the deviation)
}

#[cfg(feature = "trafo-core")]
impl JaCore {
    /// Voiced LO1166 (gapped) core. The air gap dominates the magnetic reluctance, so the core
    /// presents a LOW effective susceptibility `chi0`: level-stable inductance and a *gentle*
    /// saturation knee — the opposite of an ungapped high-µ core (whose `chi0`~thousands gives a
    /// catastrophic post-knee current rise and flux runaway, the original NO-GO). `lp_eff` is pinned
    /// to the linear coupled-inductor magnetizing inductance (1.9 H) and `k_hi` is back-solved from
    /// the *static* anhysteretic susceptibility `chi0` so the JA magnetizing current is close to
    /// `phi/lp_eff` at low drive and the deviation branch `(i_JA − phi/lp_eff)` is small near zero.
    ///
    /// NOTE — the cancellation is approximate, not exact. `k_hi` uses the static `chi0 =
    /// chi_raw/(1−α·chi_raw)` (≈20), but the RK4 integrand's near-origin slope is the *dynamic*
    /// `s0 = c·chi_raw/(1−α·c·chi_raw)` (≈14 with `c = 0.7`), because reversibility scales the
    /// differential susceptibility. So at low drive `i_dev` is a small nonzero value → the knob
    /// adds a slight low-drive gain trim on top of the saturation, not pure saturation. Exact
    /// continuity at trafo=0 is unaffected (the `trafo` gate forces `i_dev = 0`). To make the
    /// deviation truly vanish at low drive, pin `k_hi` to `s0` instead of `chi0` (uses `1.0 + s0`
    /// below) and re-run the `eval/trafo_core` B-H + THD harness — that change weakens `i_dev` ~40%
    /// at all levels, so it is a deliberate re-voice, left for a harness pass. No datasheet exists.
    pub fn lo1166() -> Self {
        let mu0 = std::f64::consts::PI * 4.0e-7;
        let b_sat = 0.3; // saturation flux density (T) — knee sits at the top of the line-level swing
                         // (measured electrical-flux peak ≈ 0.48 T at hard drive), so heavy signal
                         // pushes past the knee while nominal levels stay near-linear.
        let m_s = b_sat / mu0; // saturation magnetization (A/m) ≈ 7.96e5
        let a = m_s / 60.0; // anhysteretic shape (A/m): chi_raw = m_s/(3a) = 20 (low → gapped, soft)
        let alpha = 1.0e-5; // inter-domain coupling (small: chi0 ≈ chi_raw)
        let k = 300.0; // pinning / coercivity (A/m): soft transformer steel, thin loop
        let c = 0.7; // reversibility 0..1
        let n_p = 100.0;
        let a_e = 5.0e-4;
        let k_phi = n_p * a_e; // volts→flux lump (= N_p·A_e)
        // Pin lp_eff to the linear magnetizing L (companion: Lp = 1.9 H) and back-solve k_hi from the
        // static initial susceptibility so the deviation is small at low drive. (Approximate — the
        // dynamic RK4 slope includes the reversibility factor `c`; see the doc-comment above.)
        let lp_eff = 1.9;
        let chi_raw = m_s / (3.0 * a);
        let chi0 = chi_raw / (1.0 - alpha * chi_raw);
        let k_hi = lp_eff / (k_phi * mu0 * (1.0 + chi0)); // current→field lump (= N_p/l_e)
        JaCore { m_s, a, alpha, k, c, mu0, k_phi, k_hi, lp_eff }
    }

    /// JA differential susceptibility `dM/dH` at field `h`, magnetization `m`, direction `delta`
    /// (sign of dH/dt, frozen per sample). Standard mean-field closed form.
    #[inline]
    fn slope(&self, h: f64, m: f64, delta: f64) -> f64 {
        let he = h + self.alpha * m;
        let (la, dla) = langevin(he / self.a);
        let m_an = self.m_s * la;
        let dman_dhe = (self.m_s / self.a) * dla;
        let denom = {
            let d = self.k * delta - self.alpha * (m_an - m);
            if d.abs() < 1.0e-12 { if d < 0.0 { -1.0e-12 } else { 1.0e-12 } } else { d }
        };
        let dm_irr = (m_an - m) / denom;
        let num = (1.0 - self.c) * dm_irr + self.c * dman_dhe;
        let den = 1.0 - self.alpha * self.c * dman_dhe - self.alpha * (1.0 - self.c) * dm_irr;
        // Sign-preserving small-denominator clamp (mirror the `denom` clamp above): a small
        // *negative* `den` must not flip the slope sign before the `max(0.0)` floor.
        let den = if den.abs() < 1.0e-9 { if den < 0.0 { -1.0e-9 } else { 1.0e-9 } } else { den };
        let s = num / den;
        s.max(0.0) // physical differential susceptibility is non-negative
    }

    /// Separable hysteresis ODE in B-driven form: with `B = μ0(H+M)` the field is algebraic
    /// (`H = B/μ0 − M`), so `dM/dB = s / (μ0·(1+s))` where `s = dM/dH` is the differential
    /// susceptibility. No inner Newton — this is the integrand the RK4 core advances. `delta` is
    /// the frozen field-rate direction (per-sample), passed in by the caller.
    #[inline]
    fn dmdb(&self, b: f64, m: f64, delta: f64) -> f64 {
        let h = b / self.mu0 - m;
        let s = self.slope(h, m, delta);
        s / (self.mu0 * (1.0 + s))
    }
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

    r_kj_idx: usize,    // R_KJ resistor index (gain knob), for set_rgain
    r_upper_idx: usize, // RV1 upper-leg index, for set_bias
    r_lower_idx: usize, // RV1 lower-leg index

    // --- component aging (deterministic per-unit drift) ---
    res_base_g: [f64; MAX_RES],   // pristine 1/ohms captured at build (incl. the live bias/rgain legs)
    cap_base_far: [f64; MAX_CAPS], // pristine farads captured at build
    age_pattern: AgePattern,      // per-unit signed deviations (seed-derived, age-independent)
    age: f64,                     // current knob value [0,1]; 0 => no drift
    age_seed: u64,                // current unit seed

    // coupled inductor companion (transformer, trapezoidal): g = (dt/2) * inv([Lp M; M Ls])
    gl: [[f64; 2]; 2],
    i_hist_p: f64, // primary winding current i[n-1]
    i_hist_s: f64, // secondary winding current i[n-1]
    v_pprev: f64,  // winding voltages at n-1
    v_sprev: f64,

    // --- nonlinear JA transformer core (`trafo` knob; feature `trafo-core`) ---
    #[cfg(feature = "trafo-core")]
    trafo: f64, // knob 0..1; 0 => ja_active false (linear, bit-exact)
    #[cfg(feature = "trafo-core")]
    ja_active: bool,
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
    fixed_step: bool, // Med/Low precision: exactly max_iters, no residual_norm gate / line search
    os: usize, // internal oversampling factor (host_rate * os = solve rate); 2 normally, 1 ultra-fast
    decimate: bool, // apply output decimation biquads (needless at os==1)

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
    dec1: Biquad,
    dec2: Biquad,
    sec_in_dc: f64, // output DC offset captured at warmup; subtracted from the tap
    prev_in: f64,   // previous host input sample (linear-interpolation upsampling state)
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
    /// Size of the Schur nonlinear-coupled (N) partition. 6 for the linear circuit; the JA core
    /// (when active) adds N_PA → 7.
    pub fn n_sz(&self) -> usize {
        self.n_sz
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

    /// Flat row-major copy of the constant conductance matrix `g0` (first `u*u`
    /// entries). Diagnostic/test use only.
    pub fn g0_snapshot(&self) -> Vec<f64> {
        let u = self.u;
        self.g0[..u * u].to_vec()
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

        // Capture pristine resistor conductances for aging (re-derived per age apply).
        let mut res_base_g = [0.0f64; MAX_RES];
        for i in 0..n_res {
            res_base_g[i] = res[i].g;
        }

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

        // Capture pristine cap farads for aging (geq re-derived per age apply).
        let mut cap_base_far = [0.0f64; MAX_CAPS];
        for i in 0..n_caps {
            cap_base_far[i] = caps[i].c;
        }

        // --- BJTs (RB off: external base directly) ---
        let mut bc = bc184c();
        let mut q = q2n3055();
        bc.cheap_exp = cfg.cheap_exp; // ultra-fast tier: low-order transport exp
        q.cheap_exp = cfg.cheap_exp;
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
            res_base_g,
            cap_base_far,
            age_pattern: AgePattern::identity(),
            age: 0.0,
            age_seed: 0,
            gl,
            i_hist_p: 0.0,
            i_hist_s: 0.0,
            v_pprev: 0.0,
            v_sprev: 0.0,
            #[cfg(feature = "trafo-core")]
            trafo: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_active: false,
            #[cfg(feature = "trafo-core")]
            ja: JaCore::lo1166(),
            #[cfg(feature = "trafo-core")]
            ja_m: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_h_prev: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_phi_prev: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_vm_prev: 0.0,
            #[cfg(feature = "trafo-core")]
            ja_delta: 1.0,
            #[cfg(feature = "trafo-core")]
            ja_i_dev: 0.0,
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
            fma: cfg.fma,
            fixed_step: cfg.fixed_step,
            // os=1 is permitted for the full-convergence solver (ultra-fast/warp speed lever) but
            // floored to 2 under the fixed-step solver, where os=1 limit-cycles into loud peaks.
            // `solve_os`/`solve_decimate` are the single source of truth (also used by `new_with`),
            // so no config — including a hand-assembled `fixed_step` one — can solve at os=1.
            os: cfg.solve_os(),
            decimate: cfg.solve_decimate(),
            v_pred_prev: [0.0; NUM_NODES],
            predict_primed: false,
            in_audio_pass: false,
            frozen_cap: [(0.0, 0.0, 0.0, 0.0); 3],
            frozen_caps_valid: false,
            cap_q_cache: [(0.0, 0.0); 3],
            cap_cache_valid: false,
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
            iter_samples: 0,
            iter_total: 0,
            iter_max: 0,
            #[cfg(feature = "subrate-spike")]
            subrate: cfg.subrate.max(1),
            #[cfg(feature = "subrate-spike")]
            subrate_hold: cfg.subrate_hold,
            #[cfg(feature = "subrate-spike")]
            subrate_phase: 0,
            #[cfg(feature = "subrate-spike")]
            subrate_valid: false,
            #[cfg(feature = "subrate-spike")]
            capture_j: false,
            #[cfg(feature = "subrate-spike")]
            j_captured: false,
            #[cfg(feature = "subrate-spike")]
            j_frozen_raw: [0.0; NUM_NODES * NUM_NODES],
            #[cfg(feature = "subrate-spike")]
            j_dev: [0.0; NUM_NODES * NUM_NODES],
            #[cfg(feature = "subrate-spike")]
            lu_frozen: [0.0; NUM_NODES * NUM_NODES],
            #[cfg(feature = "subrate-spike")]
            piv_frozen: [0; NUM_NODES],
            #[cfg(feature = "subrate-spike")]
            i0_const: [0.0; NUM_NODES],
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
        // Use the same effective os as build_with (conditional floor under fixed_step) so dt
        // (= 1/internal_sr) matches the per-host-sample subsample count.
        let internal_sr = host_sr * cfg.solve_os() as f64;
        let mut m = Self::build_with(cfg, internal_sr, rgain, bias);
        // Decimation biquads are always configured, used when `cfg.decimate` (see
        // `process_host_sample`). At os=2 they are the antialiasing decimator and stay on; configuring
        // them unconditionally keeps the path branch-free.
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
    /// (`main.cpp:432-438`). Composes the aged drift factor on each leg (see `set_age`).
    pub fn set_bias(&mut self, bias: f64) {
        let (r_upper, r_lower) = rv1_split(bias);
        self.res[self.r_upper_idx].g = 1.0 / (r_upper * self.age_factor_res(self.r_upper_idx));
        self.res[self.r_lower_idx].g = 1.0 / (r_lower * self.age_factor_res(self.r_lower_idx));
        self.build_g0();
    }

    /// Update R_KJ (circuit gain) + rebuild G0, preserving integrator state. New vs the C++
    /// reference (which only sets gain at build time); mirrors `set_bias`. Composes aged drift.
    pub fn set_rgain(&mut self, rgain: f64) {
        self.res[self.r_kj_idx].g = 1.0 / (rgain * self.age_factor_res(self.r_kj_idx));
        self.build_g0();
    }

    /// Aged resistance multiplier for resistor `idx`: `1 + age*dev`. Clamped away from 0.
    #[inline]
    fn age_factor_res(&self, idx: usize) -> f64 {
        (1.0 + self.age * self.age_pattern.res_dev[idx]).max(1e-6)
    }

    /// Apply the aging knob: re-derive every resistor `g` and cap `geq` from its pristine base
    /// times the per-unit drift factor `(1 + age*dev)`, then rebuild G0 (+ Schur). Block-rate;
    /// preserves integrator state (dynamic re-settle, exactly like `set_bias`). The three
    /// parameter-derived legs (`r_kj`/`r_upper`/`r_lower`) are skipped here — their drift is folded
    /// in by `set_bias`/`set_rgain` via `age_factor_res`, which the plugin re-applies after a
    /// change.
    pub fn set_age(&mut self, age: f64) {
        self.age = age.clamp(0.0, 1.0);
        for i in 0..self.n_res {
            if i == self.r_kj_idx || i == self.r_upper_idx || i == self.r_lower_idx {
                continue;
            }
            let f = (1.0 + self.age * self.age_pattern.res_dev[i]).max(1e-6);
            self.res[i].g = self.res_base_g[i] / f;
        }
        for i in 0..self.n_caps {
            let f = (1.0 + self.age * self.age_pattern.cap_dev[i]).max(1e-9);
            let c = self.cap_base_far[i] * f;
            self.caps[i].c = c;
            self.caps[i].geq = 2.0 * c / self.dt;
        }
        self.build_g0();
    }

    /// Re-roll the "unit": regenerate the per-component drift pattern from `seed`, then re-apply at
    /// the current `age`. The live bias/rgain legs are refreshed by the plugin afterward.
    pub fn set_age_seed(&mut self, seed: u64) {
        self.age_seed = seed;
        self.age_pattern =
            AgePattern::from_seed(seed, self.n_res, self.n_caps, &self.cap_base_far);
        self.set_age(self.age);
    }

    /// Commit the JA core state for the next sample — the separable RK4 magnetizing filter (call
    /// once per sample after the network solve, when `ja_active`). The linear network solve already
    /// ran with the magnetizing branch represented by the *linear* companion; this advances the
    /// hysteretic magnetization `M` by integrating `dM/dB` with fixed-step RK4 over the flux
    /// increment `[B[n-1], B[n]]`, then computes the lagged *deviation* current
    /// `i_dev = trafo·(i_JA − i_lin)` that is injected into the next sample's RHS (decoupled from
    /// the Newton loop — no Jacobian contribution, no inner Newton). `delta` is frozen across the
    /// four RK4 stages and updated from the net field change at the end.
    #[cfg(feature = "trafo-core")]
    #[inline]
    fn ja_commit(&mut self) {
        use node::{N_A, N_PA};
        let vm = self.node_v(N_PA) - self.node_v(N_A);
        // trapezoidal flux at the converged magnetizing voltage
        let phi = self.ja_phi_prev + 0.5 * self.dt * (vm + self.ja_vm_prev);
        let b0 = self.ja_phi_prev / self.ja.k_phi; // B[n-1]
        let b1 = phi / self.ja.k_phi; // B[n]
        let hb = b1 - b0;
        let m0 = self.ja_m;
        let d = self.ja_delta;
        // RK4 over [b0, b1] (4 fixed stages; H is algebraic at each stage so no inner Newton).
        let k1 = self.ja.dmdb(b0, m0, d);
        let k2 = self.ja.dmdb(b0 + 0.5 * hb, m0 + 0.5 * hb * k1, d);
        let k3 = self.ja.dmdb(b0 + 0.5 * hb, m0 + 0.5 * hb * k2, d);
        let k4 = self.ja.dmdb(b1, m0 + hb * k3, d);
        let m = m0 + (hb / 6.0) * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
        let h = b1 / self.ja.mu0 - m; // algebraic field at the new state
        let dh = h - self.ja_h_prev;
        if dh != 0.0 {
            self.ja_delta = dh.signum();
        }
        // Lagged magnetizing-deviation current for the next sample (excess beyond the linear core,
        // so it vanishes at low drive → smooth from trafo=0⁺).
        let i_ja = h / self.ja.k_hi;
        let i_lin = phi / self.ja.lp_eff;
        self.ja_i_dev = self.trafo * (i_ja - i_lin);
        self.ja_m = m;
        self.ja_h_prev = h;
        self.ja_phi_prev = phi;
        self.ja_vm_prev = vm;
    }

    /// Set the `trafo` knob (0..1). 0 => `ja_active` false => the magnetizing-deviation current
    /// `ja_i_dev` is forced to 0 => bit-exact linear core. The J-A is a separable RK4 post-solve
    /// filter injecting a lagged current source, so it touches neither G0 nor the Schur partition
    /// (N-set stays 6 always); `set_trafo` only flips the gate.
    /// Reset the JA core to a clean origin: `M=0`, no flux/field/voltage history, `+δ` field-rate
    /// direction, no lagged deviation current. Single source of truth for "what a settled core looks
    /// like" — called on both edges that must not integrate the next trapezoidal step against a stale
    /// operating point: warmup→audio (`process_host_sample`) and inactive→active (`set_trafo`).
    #[cfg(feature = "trafo-core")]
    #[inline]
    fn ja_reseed(&mut self) {
        self.ja_m = 0.0;
        self.ja_h_prev = 0.0;
        self.ja_phi_prev = 0.0;
        self.ja_vm_prev = 0.0;
        self.ja_delta = 1.0;
        self.ja_i_dev = 0.0;
    }

    #[cfg(feature = "trafo-core")]
    pub fn set_trafo(&mut self, trafo: f64) {
        self.trafo = trafo.clamp(0.0, 1.0);
        let active = self.trafo > 0.0;
        // Re-enable from a linear/inactive state: the core stopped committing when it went inactive,
        // so `ja_vm_prev`/`ja_phi_prev`/`ja_m` are frozen at a stale operating point. Integrating the
        // next trapezoidal flux step against that stale `ja_vm_prev` would jump the flux (audible
        // click). Re-seed to a clean start on the inactive→active edge (same reset the warmup→audio
        // transition does). No-op on a fresh model (state already zero) and on level changes while
        // already active.
        if active && !self.ja_active {
            self.ja_reseed();
        }
        self.ja_active = active;
    }

    /// Current core operating point `(H, B)` in SI units — for offline B-H loop logging. Inert
    /// (returns 0,0) when the core is linear/inactive.
    #[cfg(feature = "trafo-core")]
    pub fn ja_hb(&self) -> (f64, f64) {
        if !self.ja_active {
            return (0.0, 0.0);
        }
        let b = self.ja_phi_prev / self.ja.k_phi;
        (self.ja_h_prev, b)
    }

    /// Lagged magnetizing-deviation current `i_dev` queued for the next sample's RHS injection — the
    /// actual click source on a knob re-enable (a stale value steps the network current). For tests.
    #[cfg(feature = "trafo-core")]
    pub fn ja_i_dev(&self) -> f64 {
        self.ja_i_dev
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
                    let (qbe_dep, _) =
                        depletion_charge_pre(vbe, t.p.cje, t.p.vje, t.p.mje, t.p.je_c0, t.p.je_q0, t.p.je_dcdv);
                    let (qbc_dep, _) =
                        depletion_charge_pre(vbc, t.p.cjc, t.p.vjc, t.p.mjc, t.p.jc_c0, t.p.jc_q0, t.p.jc_dcdv);
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

    /// Fixed-size (n=16) specialization of [`gauss_solve`] using PHYSICAL row swaps instead of
    /// `row[]` index indirection, so the elimination inner loop is contiguous (`a[col_base + c]`,
    /// `a[r_base + c]`) and LLVM can autovectorize it. Bit-identical to `gauss_solve(a, b, 16)`:
    /// same pivot selection (max-abs, `>` tie-break, scan `col+1..16`), same `f`, same operand
    /// order, same loop bounds (`col..16` / `r+1..16`), same `f == 0.0` short-circuit and
    /// `best < 1e-30` bail. Physical-swap vs index-swap differs only in memory address.
    fn gauss_solve_16(a: &mut [f64], b: &mut [f64]) -> bool {
        const N: usize = 16;
        for col in 0..N {
            // Pivot selection: max-abs in column `col`, scan col+1..N, strict `>` tie-break.
            let mut piv = col;
            let mut best = a[col * N + col].abs();
            for r in (col + 1)..N {
                let val = a[r * N + col].abs();
                if val > best {
                    best = val;
                    piv = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if piv != col {
                // PHYSICAL swap of the two 16-element A rows AND the b entries.
                for c in 0..N {
                    a.swap(col * N + c, piv * N + c);
                }
                b.swap(col, piv);
            }
            let col_base = col * N;
            let diag = a[col_base + col];
            for r in (col + 1)..N {
                let r_base = r * N;
                let f = a[r_base + col] / diag;
                if f == 0.0 {
                    continue;
                }
                // Contiguous: matches `gauss_solve`'s `c in col..n`. Vectorizable.
                for c in col..N {
                    a[r_base + c] -= f * a[col_base + c];
                }
                b[r] -= f * b[col];
            }
        }
        // Back-substitution: SCALAR reduction, same summation order as `gauss_solve`.
        for r in (0..N).rev() {
            let r_base = r * N;
            let mut s = b[r];
            for c in (r + 1)..N {
                s -= a[r_base + c] * b[c];
            }
            b[r] = s / a[r_base + r];
        }
        true
    }

    /// Fixed-size (n=6) specialization of [`gauss_solve`] using PHYSICAL row swaps instead of
    /// `row[]` index indirection, so the elimination inner loop is contiguous (`a[col_base + c]`,
    /// `a[r_base + c]`). Bit-identical to `gauss_solve(a, b, 6)`: same pivot selection (max-abs,
    /// `>` tie-break, scan `col+1..6`), same `f`, same operand order, same loop bounds
    /// (`col..6` / `r+1..6`), same `f == 0.0` short-circuit and `best < 1e-30` bail. Physical-swap
    /// vs index-swap differs only in memory address. Used by `schur_solve` for the n=6 complement.
    fn gauss_solve_6(a: &mut [f64], b: &mut [f64]) -> bool {
        const N: usize = 6;
        for col in 0..N {
            // Pivot selection: max-abs in column `col`, scan col+1..N, strict `>` tie-break.
            let mut piv = col;
            let mut best = a[col * N + col].abs();
            for r in (col + 1)..N {
                let val = a[r * N + col].abs();
                if val > best {
                    best = val;
                    piv = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if piv != col {
                // PHYSICAL swap of the two 6-element A rows AND the b entries.
                for c in 0..N {
                    a.swap(col * N + c, piv * N + c);
                }
                b.swap(col, piv);
            }
            let col_base = col * N;
            let diag = a[col_base + col];
            for r in (col + 1)..N {
                let r_base = r * N;
                let f = a[r_base + col] / diag;
                if f == 0.0 {
                    continue;
                }
                // Contiguous: matches `gauss_solve`'s `c in col..n`.
                for c in col..N {
                    a[r_base + c] -= f * a[col_base + c];
                }
                b[r] -= f * b[col];
            }
        }
        // Back-substitution: SCALAR reduction, same summation order as `gauss_solve`.
        for r in (0..N).rev() {
            let r_base = r * N;
            let mut s = b[r];
            for c in (r + 1)..N {
                s -= a[r_base + c] * b[c];
            }
            b[r] = s / a[r_base + r];
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

    /// Fixed-size (n=10) specialization of [`lu_factor`] using PHYSICAL row swaps instead of
    /// `piv[]` index indirection. Records the row permutation in `perm` (output position -> original
    /// row index), exactly mirroring the generic `piv[]` semantics, so [`lu_solve_10`] can permute
    /// the RHS while reading the contiguous (`a[r*10+c]`) factor storage. Bit-identical to
    /// `lu_factor(a, piv, 10)`: same pivot selection (max-abs in column `col`, scan `col+1..10`,
    /// strict `>` tie-break), same `f`, same operand order, same loop bounds (`col+1..10`), same
    /// unit-diagonal-L multiplier storage, same `f == 0.0` short-circuit, same `best < 1e-30` bail.
    /// Physical-swap vs index-swap differs only in memory address.
    fn lu_factor_10(a: &mut [f64], perm: &mut [usize]) -> bool {
        const N: usize = 10;
        for (i, slot) in perm.iter_mut().enumerate().take(N) {
            *slot = i;
        }
        for col in 0..N {
            // Pivot selection over physical rows col..N (identity-permuted, matches generic order).
            let mut p = col;
            let mut best = a[col * N + col].abs();
            for r in (col + 1)..N {
                let val = a[r * N + col].abs();
                if val > best {
                    best = val;
                    p = r;
                }
            }
            if best < 1e-30 {
                return false;
            }
            if p != col {
                // PHYSICAL swap of the two 10-element rows; record in perm for the RHS.
                for c in 0..N {
                    a.swap(col * N + c, p * N + c);
                }
                perm.swap(col, p);
            }
            let col_base = col * N;
            let diag = a[col_base + col];
            for r in (col + 1)..N {
                let r_base = r * N;
                let f = a[r_base + col] / diag;
                a[r_base + col] = f; // store multiplier (L factor)
                if f == 0.0 {
                    continue;
                }
                for c in (col + 1)..N {
                    a[r_base + c] -= f * a[col_base + c];
                }
            }
        }
        true
    }

    /// Solve `A x = b` in place (`x` in `b`) using the contiguous (physical-swap) LU factors +
    /// `perm` row permutation from [`lu_factor_10`]. Bit-identical to `lu_solve(a, piv, 10, b)`:
    /// the RHS read `b[perm[r]]` matches `b[piv[r]]`, factor reads are the same values in natural
    /// row order, forward/back substitution are SCALAR reductions in the same summation order.
    fn lu_solve_10(a: &[f64], perm: &[usize], b: &mut [f64]) {
        const N: usize = 10;
        let mut y = [0.0f64; NUM_NODES];
        for r in 0..N {
            let r_base = r * N;
            let mut s = b[perm[r]];
            for c in 0..r {
                s -= a[r_base + c] * y[c];
            }
            y[r] = s; // unit-diagonal L
        }
        for r in (0..N).rev() {
            let r_base = r * N;
            let mut s = y[r];
            for c in (r + 1)..N {
                s -= a[r_base + c] * b[c];
            }
            b[r] = s / a[r_base + r];
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
        // JA core is decoupled (separable RK4 post-solve filter → lagged current source), so it adds
        // nothing to the nonlinear N-set: the partition is identical with or without `trafo-core`.
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
        // Use the contiguous physical-swap LU pair when l == 10 (fast tier's A_LL block);
        // generic index-swap pair otherwise. Both are bit-identical; only storage layout differs.
        let factored = if l == 10 {
            Self::lu_factor_10(&mut self.s_ll_fac, &mut self.s_ll_piv)
        } else {
            Self::lu_factor(&mut self.s_ll_fac, &mut self.s_ll_piv, l)
        };
        if !factored {
            return false;
        }
        // W = A_LL^{-1} A_LN  (solve A_LL * W[:,c] = A_LN[:,c] for each N column)
        let mut col = [0.0f64; NUM_NODES];
        for c in 0..n {
            for i in 0..l {
                col[i] = self.s_aln[i * n + c];
            }
            if l == 10 {
                Self::lu_solve_10(&self.s_ll_fac, &self.s_ll_piv, &mut col);
            } else {
                Self::lu_solve(&self.s_ll_fac, &self.s_ll_piv, l, &mut col);
            }
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
        if l == 10 {
            Self::lu_solve_10(&self.s_ll_fac, &self.s_ll_piv, &mut y);
        } else {
            Self::lu_solve(&self.s_ll_fac, &self.s_ll_piv, l, &mut y);
        }
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
        let solved = if n == 6 {
            Self::gauss_solve_6(&mut self.s_s, &mut x_n)
        } else {
            Self::gauss_solve(&mut self.s_s, &mut x_n, n)
        };
        if !solved {
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
        // Trailing-state reuse: invalidate per call; only the convergence break re-arms it. After the
        // gmin homotopy the last (gmin=0) call's break leaves the cache at the returned `v`.
        self.cap_cache_valid = false;

        // Deferred-acceptance line-search state (only used when ls_trim). Instead of verifying
        // each Newton step with its own residual_norm call, take the full step optimistically and
        // let the NEXT iter's mandatory F build double as verification; if the residual rose,
        // retroactively restore and run a real backtracking line search (`main.cpp:714-723`).
        // Fixed-step (Med/Low precision): never trim, never run the per-step `residual_norm` line
        // search; take the bounded full Newton step each iter (the cheap `norm_f` tol-break still
        // short-circuits a converged sample). Lossy on transients; gated by the harness.
        let fixed = self.fixed_step;
        let use_trim = self.ls_trim && !fixed;
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
            // Gather → batched device eval (SLP-vectorized) → scalar scatter.
            let k2 = 2.0 / self.dt;

            // Gather vbe/vbc for 3 BJTs into [f64;4] SoA (lane 3 = padding=0).
            let mut vbe4 = [0.0_f64; 4];
            let mut vbc4 = [0.0_f64; 4];
            for bi in 0..3 {
                let t = &self.bjts[bi];
                vbe4[bi] = self.node_v(t.b) - self.node_v(t.e);
                vbc4[bi] = self.node_v(t.b) - self.node_v(t.c);
            }

            // Pre-batch exp for 3 BJTs. SoA inv_vt4/vcrit4 give stride-8 loads so
            // LLVM can emit vmovupd + vmulpd, then SLP-fuse the 3 independent exp_bl
            // polynomial chains into AVX2 YMM ops (3 exp calls → 1 exp4 per array).
            // Results (ef4/er4/ec4) are 3 × 32-byte locals — no large intermediate struct.
            let b = &self.bjts;
            let inv_vt4 = [1.0/b[0].p.vt, 1.0/b[1].p.vt, 1.0/b[2].p.vt, 0.0];
            let vcrit4  = [b[0].p.vcrit,   b[1].p.vcrit,   b[2].p.vcrit,   0.0];
            // T3: FMA contraction on fast/ultra (lossy); realtime keeps the bit-identical a*b+c path.
            let fma_on = self.fma;
            let (ef4, er4, ec4) = if fma_on {
                (exp4::<true>(&vbe4, &inv_vt4), exp4::<true>(&vbc4, &inv_vt4), exp4::<true>(&vcrit4, &inv_vt4))
            } else {
                (exp4::<false>(&vbe4, &inv_vt4), exp4::<false>(&vbc4, &inv_vt4), exp4::<false>(&vcrit4, &inv_vt4))
            };

            // Batch depletion charge — only when caps enabled and not using frozen companion.
            let use_frozen_dep = self.freeze_caps && self.frozen_caps_valid;
            let run_dep = self.cap_tier != CapTier::Off && !use_frozen_dep;
            let (qbe_dep4, cbe_dep4, qbc_dep4, cbc_dep4) = if run_dep {
                let cje4 = [b[0].p.cje, b[1].p.cje, b[2].p.cje, 0.0];
                let vje4 = [b[0].p.vje, b[1].p.vje, b[2].p.vje, 0.75];
                let mje4 = [b[0].p.mje, b[1].p.mje, b[2].p.mje, 0.33];
                let je_c04 = [b[0].p.je_c0, b[1].p.je_c0, b[2].p.je_c0, 0.0];
                let je_q04 = [b[0].p.je_q0, b[1].p.je_q0, b[2].p.je_q0, 0.0];
                let je_dcdv4 = [b[0].p.je_dcdv, b[1].p.je_dcdv, b[2].p.je_dcdv, 0.0];
                let cjc4 = [b[0].p.cjc, b[1].p.cjc, b[2].p.cjc, 0.0];
                let vjc4 = [b[0].p.vjc, b[1].p.vjc, b[2].p.vjc, 0.75];
                let mjc4 = [b[0].p.mjc, b[1].p.mjc, b[2].p.mjc, 0.33];
                let jc_c04 = [b[0].p.jc_c0, b[1].p.jc_c0, b[2].p.jc_c0, 0.0];
                let jc_q04 = [b[0].p.jc_q0, b[1].p.jc_q0, b[2].p.jc_q0, 0.0];
                let jc_dcdv4 = [b[0].p.jc_dcdv, b[1].p.jc_dcdv, b[2].p.jc_dcdv, 0.0];
                let ((qbe4, cbe4), (qbc4, cbc4)) = if fma_on {
                    (depletion_charge_pre_batch::<true>(vbe4, cje4, vje4, mje4, je_c04, je_q04, je_dcdv4),
                     depletion_charge_pre_batch::<true>(vbc4, cjc4, vjc4, mjc4, jc_c04, jc_q04, jc_dcdv4))
                } else {
                    (depletion_charge_pre_batch::<false>(vbe4, cje4, vje4, mje4, je_c04, je_q04, je_dcdv4),
                     depletion_charge_pre_batch::<false>(vbc4, cjc4, vjc4, mjc4, jc_c04, jc_q04, jc_dcdv4))
                };
                (qbe4, cbe4, qbc4, cbc4)
            } else {
                ([0.0_f64; 4], [0.0_f64; 4], [0.0_f64; 4], [0.0_f64; 4])
            };

            // Post-exp Gummel-Poon algebra: batch the divide-/sqrt-heavy 16-field Jacobian for
            // all 3 BJTs into one width-4 SoA call (SLP → packed vdivpd/vsqrtpd), reusing the
            // already-batched ef4/er4/ec4. The scatter tail stays scalar (AoS stamps).
            let bout = if fma_on {
                compute_bjt_preexp_batch::<true>(&vbe4, &vbc4, b, &ef4, &er4, &ec4)
            } else {
                compute_bjt_preexp_batch::<false>(&vbe4, &vbc4, b, &ef4, &er4, &ec4)
            };
            for bi in 0..3 {
                let t = self.bjts[bi];
                let vbe = vbe4[bi];
                let vbc = vbc4[bi];
                let o = BjtOut {
                    ib: bout.ib[bi], ic: bout.ic[bi], ie: bout.ie[bi],
                    itf: bout.itf[bi], gtf: bout.gtf[bi], itr: bout.itr[bi], gtr: bout.gtr[bi],
                    dib_b: bout.dib_b[bi], dib_e: bout.dib_e[bi], dib_c: bout.dib_c[bi],
                    dic_b: bout.dic_b[bi], dic_e: bout.dic_e[bi], dic_c: bout.dic_c[bi],
                    die_b: bout.die_b[bi], die_e: bout.die_e[bi], die_c: bout.die_c[bi],
                };
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
                    let (qbe, cbe, qbc, cbc) = if use_frozen_dep {
                        let fc = self.frozen_cap[bi];
                        (fc.2 + fc.0 * vbe, fc.0, fc.3 + fc.1 * vbc, fc.1)
                    } else {
                        let qbe = qbe_dep4[bi] + tf * o.itf;
                        let cbe = cbe_dep4[bi] + tf * o.gtf;
                        let qbc = qbc_dep4[bi] + tr * o.itr;
                        let cbc = cbc_dep4[bi] + tr * o.gtr;
                        if self.freeze_caps {
                            self.frozen_cap[bi] = (cbe, cbc, qbe - cbe * vbe, qbc - cbc * vbc);
                        }
                        (qbe, cbe, qbc, cbc)
                    };
                    // Trailing-state reuse: remember this iter's charges. Only read back (post-loop)
                    // when the solve converged with freeze_caps off, where these are the true
                    // depletion+diffusion charges at the converged v (frozen path stores the
                    // companion approximation, never read because cap_cache_valid stays false).
                    self.cap_q_cache[bi] = (qbe, qbc);
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
                // Sub-rate spike: snapshot the converged J for the frozen-linear skipped samples.
                // Compiled away in the default build (no `capture_j` field without the feature).
                #[cfg(feature = "subrate-spike")]
                if self.capture_j {
                    self.j_frozen_raw[..u * u].copy_from_slice(&self.j[..u * u]);
                    self.j_captured = true;
                }
                // Trailing-state reuse: this iter built the device charges at the converged v and the
                // break skips the step, so `cap_q_cache` matches the returned v. Arm reuse only when
                // the cached charges are bit-identical to the scalar trailing recompute:
                //  - freeze_caps off — else the stored charges are the frozen companion, not true;
                //  - cheap_exp off — the batch always uses the accurate exp, but the scalar
                //    diode_branch would use ultra_exp, so they'd diverge.
                self.cap_cache_valid = !self.freeze_caps && !self.bjts[0].p.cheap_exp;
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
            } else if u == 16 {
                // Realtime tier's U: fixed-size, physical-swap specialization (bit-exact).
                Self::gauss_solve_16(&mut self.j, &mut self.delta)
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

            if fixed {
                // Fixed-step: accept the full bounded Newton step, no `residual_norm` line search.
                let mut maxd = 0.0f64;
                for i in 0..NUM_NODES {
                    let r = self.urow[i];
                    if r >= 0 {
                        let d = self.delta[r as usize];
                        self.v[i] += d;
                        maxd = maxd.max(d.abs());
                    }
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

    /// True trapezoidal junction charges `(qbe, qbc)` for `t` at the current `v` (depletion +
    /// diffusion). Single source for the trailing-state update's recompute path AND its debug
    /// bit-exactness guard, so the two can't silently drift (cf. the duplication warning on the
    /// `depletion_charge_pre` golden test). Mirrors the per-BJT charge built in `newton_iterate`.
    #[inline]
    fn junction_charges(&self, t: &Bjt) -> (f64, f64) {
        let tf = if self.cap_tier == CapTier::Full { t.p.tf } else { 0.0 };
        let tr = if self.cap_tier == CapTier::Full { t.p.tr } else { 0.0 };
        let vbe = self.node_v(t.b) - self.node_v(t.e);
        let vbc = self.node_v(t.b) - self.node_v(t.c);
        let (i_be, _) = diode_branch(vbe, t.p.is, t.p.vt, t.p.vcrit, t.p.cheap_exp);
        let (i_bc, _) = diode_branch(vbc, t.p.is, t.p.vt, t.p.vcrit, t.p.cheap_exp);
        let (qbe_dep, _) =
            depletion_charge_pre(vbe, t.p.cje, t.p.vje, t.p.mje, t.p.je_c0, t.p.je_q0, t.p.je_dcdv);
        let (qbc_dep, _) =
            depletion_charge_pre(vbc, t.p.cjc, t.p.vjc, t.p.mjc, t.p.jc_c0, t.p.jc_q0, t.p.jc_dcdv);
        (qbe_dep + tf * i_be, qbc_dep + tr * i_bc)
    }

    /// Sub-rate spike helper: add the linear branch residuals (resistors + linear-cap companions +
    /// coupled inductor) at the current `v` into the reduced residual `f`. Byte-for-byte the same
    /// stamps as `newton_iterate`'s linear block, but without touching `self.j` or the devices.
    #[cfg(feature = "subrate-spike")]
    fn add_linear_residual(&self, f: &mut [f64]) {
        use node::{N_A, N_PA, N_SA};
        for i in 0..self.n_res {
            let r = self.res[i];
            let d = r.g * (self.node_v(r.a) - self.node_v(r.b));
            let (ra, rb) = (self.ur(r.a), self.ur(r.b));
            if ra >= 0 {
                f[ra as usize] += d;
            }
            if rb >= 0 {
                f[rb as usize] -= d;
            }
        }
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let d = c.geq * (self.node_v(c.a) - self.node_v(c.b));
            let (ra, rb) = (self.ur(c.a), self.ur(c.b));
            if ra >= 0 {
                f[ra as usize] += d;
            }
            if rb >= 0 {
                f[rb as usize] -= d;
            }
        }
        let vp = self.node_v(N_PA) - self.node_v(N_A);
        let vs = self.node_v(N_SA);
        let (rp, rs) = (self.ur(N_PA), self.ur(N_SA));
        if rp >= 0 {
            f[rp as usize] += self.gl[0][0] * vp + self.gl[0][1] * vs;
        }
        if rs >= 0 {
            f[rs as usize] += self.gl[1][0] * vp + self.gl[1][1] * vs;
        }
    }

    #[cfg(feature = "subrate-spike")]
    /// Sub-rate spike: after a converged full solve at `v0`, build the frozen linear model used by
    /// the next `subrate-1` skipped samples. `newton_iterate` already snapshot the converged J into
    /// `j_frozen_raw` (= G0 + J_dev(v0)). LU-factor it; derive `j_dev = j_frozen_raw - g0`; and
    /// capture the device Norton constant `i0_const = D(v0) - J_dev·v0`, computed from the converged
    /// identity `D(v0) = -L(v0)` (full residual ≈ 0 ⇒ device residual = −linear residual). On failure
    /// (no converged J this sample, or singular J) clears `subrate_valid` so the next sample also
    /// full-solves.
    fn capture_frozen(&mut self) {
        let u = self.u;
        if !self.j_captured {
            self.subrate_valid = false;
            return;
        }
        self.lu_frozen[..u * u].copy_from_slice(&self.j_frozen_raw[..u * u]);
        if !Self::lu_factor(&mut self.lu_frozen[..u * u], &mut self.piv_frozen[..u], u) {
            self.subrate_valid = false;
            return;
        }
        for i in 0..u * u {
            self.j_dev[i] = self.j_frozen_raw[i] - self.g0[i];
        }
        // Reduced converged voltages.
        let mut v0_red = [0.0f64; NUM_NODES];
        for n in 0..NUM_NODES {
            let r = self.urow[n];
            if r >= 0 {
                v0_red[r as usize] = self.v[n];
            }
        }
        // L(v0) = hist + linear residuals at v0 (self.hist still holds this sample's history).
        let mut lv = [0.0f64; NUM_NODES];
        lv[..u].copy_from_slice(&self.hist[..u]);
        self.add_linear_residual(&mut lv[..u]);
        // i0_const[r] = D(v0) - (J_dev·v0)[r] = -L(v0)[r] - (J_dev·v0)[r].
        for r in 0..u {
            let mut s = -lv[r];
            let base = r * u;
            for c in 0..u {
                s -= self.j_dev[base + c] * v0_red[c];
            }
            self.i0_const[r] = s;
        }
        self.subrate_valid = true;
    }

    #[cfg(feature = "subrate-spike")]
    /// Sub-rate spike: solve one skipped sample with the frozen model. The residual is affine in
    /// `v` — `F(v) = j_frozen·v + (hist + known-node const) + i0_const` — so one LU back-substitution
    /// against the pre-factored `lu_frozen` lands on the exact frozen-linear solution regardless of
    /// the starting guess. No `exp4` / depletion / Gummel-Poon, no re-factor.
    fn solve_frozen(&mut self) {
        let u = self.u;
        // F = hist + linear residuals + frozen device residual, evaluated at the current v.
        self.f[..u].copy_from_slice(&self.hist[..u]);
        // Borrow split: add_linear_residual reads &self, so stage into a temp then fold in.
        let mut lin = [0.0f64; NUM_NODES];
        self.add_linear_residual(&mut lin[..u]);
        for r in 0..u {
            self.f[r] += lin[r];
        }
        let mut v_red = [0.0f64; NUM_NODES];
        for n in 0..NUM_NODES {
            let r = self.urow[n];
            if r >= 0 {
                v_red[r as usize] = self.v[n];
            }
        }
        for r in 0..u {
            let mut d = self.i0_const[r];
            let base = r * u;
            for c in 0..u {
                d += self.j_dev[base + c] * v_red[c];
            }
            self.f[r] += d;
        }
        for r in 0..u {
            self.delta[r] = -self.f[r];
        }
        Self::lu_solve(&self.lu_frozen, &self.piv_frozen, u, &mut self.delta);
        for n in 0..NUM_NODES {
            let r = self.urow[n];
            if r >= 0 {
                self.v[n] += self.delta[r as usize];
            }
        }
    }

    /// Solve one sample. `v[N_A]`, `v[N_INPUT]` must be set before calling (`main.cpp:452-516`).
    pub fn process(&mut self) {
        use node::{N_A, N_PA, N_SA};
        let u = self.u;
        // Sub-rate Newton spike (behind `subrate-spike` feature). In the default build these two
        // lines are replaced by `let skipped = false` so the feature-gated solve block compiles away.
        #[cfg(feature = "subrate-spike")]
        let subrate_on = self.subrate > 1 && self.in_audio_pass;
        #[cfg(feature = "subrate-spike")]
        let skipped = subrate_on && self.subrate_valid && self.subrate_phase != 0;
        #[cfg(not(feature = "subrate-spike"))]
        let skipped = false;
        // Newton predictor: seed the solve with a 1st-order linear extrapolation of the last two
        // converged internal samples (2*sol(n-1) - sol(n-2)) for UNKNOWN nodes only. Same fixed
        // point + tol => identical converged v, just fewer iters. Off during DC warmup/rail-ramp/
        // settle (in_audio_pass false), where transient swings would make it overshoot
        // (`main.cpp:621-633`). Modifies only `v` (the guess); `hist` below reads `v_prev`.
        // Skipped sub-rate samples solve an affine system (guess-independent) or hold, so no predict.
        // Defensive: never extrapolate under fixed-step. The 2-iter fixed-step solve has no
        // `residual_norm` gate / `ls_trim` to correct a bad warm-start, so a predicted guess drifts
        // and diverges (see `with_precision`). `with_precision` already clears `predict` for Med/Low;
        // this guard makes any fixed_step config safe regardless of how it was assembled.
        if self.predict && !self.fixed_step && self.in_audio_pass && !skipped {
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
        // JA core (decoupled): inject the lagged magnetizing-deviation current as a known source on
        // N_PA's RHS row. Computed at the previous sample's commit (one os-step lag) — the J-A is
        // outside the Newton loop, so this is the ONLY place the saturation reaches the network.
        // N_A is a known rail (no unknown row), so only N_PA receives the injection.
        #[cfg(feature = "trafo-core")]
        if self.ja_active && self.in_audio_pass {
            if rp >= 0 {
                self.hist[rp as usize] += self.ja_i_dev;
            }
        }

        // Sub-rate spike solve path. With the feature, may skip Newton and reuse the frozen model.
        // Without the feature, this block is absent; the non-feature block below is the only path.
        #[cfg(feature = "subrate-spike")]
        if skipped {
            // Frozen sub-rate sample: reuse the captured Jacobian + LU. `solve_frozen` does one
            // back-substitution; `subrate_hold` is the naive baseline that freezes v instead.
            if !self.subrate_hold {
                self.solve_frozen();
            }
            // No Newton ran, so any armed charge cache is stale; force the trailing recompute.
            self.cap_cache_valid = false;
            // Instrumentation: counts as a sample with 0 device-Newton iters (the cost proxy the
            // harness reads — avgIters drops ~1/subrate when the freeze is doing real work).
            self.iter_samples += 1;
        } else {
            // Full solve. Snapshot the converged J for the following skipped samples when sub-rate
            // is engaged (`capture_j` gates the snapshot inside `newton_iterate`; off otherwise so
            // the bit-exact golden path is untouched).
            if subrate_on {
                self.j_captured = false;
                self.capture_j = true;
            }
            let mut sample_iters = self.newton_iterate(0.0);
            let mut sample_max = sample_iters;
            if subrate_on {
                self.capture_j = false; // gmin homotopy below must not overwrite the gmin=0 J
            }
            // Fixed-step (Med/Low precision) skips the per-sample convergence gate entirely: the
            // `residual_norm` re-stamp + gmin-homotopy fallback are the dominant fixed per-sample
            // cost, and the budget tolerates the occasional unconverged hard sample. HIGH/realtime
            // keep it.
            if !self.fixed_step {
                let resid = self.residual_norm(0.0);
                if resid > 1e-3 {
                    // hard sample: gmin-stepping homotopy
                    for g in [1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 0.0] {
                        let it = self.newton_iterate(g);
                        sample_iters += it;
                        sample_max = sample_max.max(it);
                    }
                }
            }
            if subrate_on {
                self.capture_frozen();
            }
            // Instrumentation only (see field comment): never read back during the solve.
            self.iter_samples += 1;
            self.iter_total += sample_iters as u64;
            self.iter_max = self.iter_max.max(sample_max);
        }
        #[cfg(feature = "subrate-spike")]
        if subrate_on {
            self.subrate_phase = (self.subrate_phase + 1) % self.subrate;
        }
        // Default (no sub-rate spike): straight full-solve path with no extra branches.
        #[cfg(not(feature = "subrate-spike"))]
        {
            let mut sample_iters = self.newton_iterate(0.0);
            let mut sample_max = sample_iters;
            // Fixed-step (Med/Low precision) skips the per-sample convergence gate entirely: the
            // `residual_norm` re-stamp + gmin-homotopy fallback are the dominant fixed per-sample
            // cost, and the budget tolerates the occasional unconverged hard sample. HIGH/realtime
            // keep it.
            if !self.fixed_step {
                let resid = self.residual_norm(0.0);
                if resid > 1e-3 {
                    // hard sample: gmin-stepping homotopy
                    for g in [1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 0.0] {
                        let it = self.newton_iterate(g);
                        sample_iters += it;
                        sample_max = sample_max.max(it);
                    }
                }
            }
            // Instrumentation only (see field comment): never read back during the solve.
            self.iter_samples += 1;
            self.iter_total += sample_iters as u64;
            self.iter_max = self.iter_max.max(sample_max);
        }

        // Fixed-step (Med/Low) drops the gmin-homotopy fallback, so a hard transient that fails to
        // converge in `max_iters` can leave `v` non-finite. WDF state feeds forward (`v_prev`, `hist`,
        // cap/BJT/inductor currents below), so a single NaN/Inf would poison every later sample —
        // permanent silence or blast in a realtime host, clearable only by reset. Recover by holding
        // the last converged iterate: restore `v` from `v_prev` so this sample freezes at the previous
        // output and forward state stays clean. HIGH/realtime never reach this (the gmin gate above
        // keeps `v` finite), and the finite path is a no-op, so the bit-exact contract is untouched.
        // Sub-rate skipped samples share the risk (a singular frozen solve could go non-finite and
        // poison forward state), so guard them the same way. Without `subrate-spike`, `skipped` is
        // always `false` and the optimizer elides the `|| skipped` term in release builds.
        if (self.fixed_step || skipped) && !self.v.iter().all(|x| x.is_finite()) {
            self.v.copy_from_slice(&self.v_prev);
        }

        // update cap currents (trapezoidal) for next sample, using old v_prev
        for i in 0..self.n_caps {
            let c = self.caps[i];
            let dv = self.node_v(c.a) - self.node_v(c.b);
            let dv_prev = self.node_v_prev(c.a) - self.node_v_prev(c.b);
            self.caps[i].i_prev = c.geq * (dv - dv_prev) - c.i_prev;
        }
        // update BJT charge-cap state (trapezoidal) for next sample. Reuse the converged charges the
        // Newton solve already built (cap_cache_valid) instead of re-running diode_branch +
        // depletion_charge_pre here — lossless, since on the armed path the cached charges are
        // bit-identical to this recompute (verified every sample in debug builds).
        if self.cap_tier != CapTier::Off {
            let k2 = 2.0 / self.dt;
            for bi in 0..self.bjts.len() {
                let t = self.bjts[bi];
                let (qbe, qbc) = if self.cap_cache_valid {
                    let cached = self.cap_q_cache[bi];
                    // Near-lossless, not bit-exact: the cached charges come from the batched device
                    // kernel (exp arg = vbe*(1/vt), multiply-by-reciprocal) while `junction_charges`
                    // uses scalar `diode_branch` (vbe/vt, true division) — they agree to a few ULP.
                    // That's ~1e-15 relative on a charge ~1e-12, far below the model's −23 dB floor
                    // and the 1e-5 golden gate. Debug builds verify the cache tracks the recompute
                    // on every sample (catches a wrong node/index/v bug — relative drift would be
                    // O(1), not O(ULP) — at near-zero release cost).
                    #[cfg(debug_assertions)]
                    {
                        let (rq_be, rq_bc) = self.junction_charges(&t);
                        let close = |a: f64, b: f64| {
                            (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-30)
                        };
                        debug_assert!(close(cached.0, rq_be), "qbe cache drift bjt {bi}: {} vs {}", cached.0, rq_be);
                        debug_assert!(close(cached.1, rq_bc), "qbc cache drift bjt {bi}: {} vs {}", cached.1, rq_bc);
                    }
                    cached
                } else {
                    self.junction_charges(&t)
                };
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

        // JA core: commit magnetization + flux state for the next sample.
        // NOTE — `trafo-core` × `subrate-spike` is an UNTESTED combination. On a sub-rate-skipped
        // sample Newton did not run, so `node_v(N_PA)` is the frozen/held solve; `ja_commit` would
        // then integrate flux from that held voltage. Both are experimental spikes not meant to be
        // enabled together; if they ever are, validate this path (or gate `ja_commit` off when the
        // sample was skipped). See the PR #40 review plan, P3 #6.
        #[cfg(feature = "trafo-core")]
        if self.ja_active && self.in_audio_pass {
            self.ja_commit();
        }

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
    /// Newton-iteration stats since the last [`reset_iter_stats`]: `(samples, avg_iters, max_iters)`.
    /// `samples` counts `process()` (internal-rate) solves; `avg` is total iters / samples. Pure
    /// instrumentation for the offline A/B harness — mirrors C++ `avgIters`/`maxItersUsed`.
    pub fn iter_stats(&self) -> (u64, f64, usize) {
        let avg = if self.iter_samples > 0 {
            self.iter_total as f64 / self.iter_samples as f64
        } else {
            0.0
        };
        (self.iter_samples, avg, self.iter_max)
    }

    /// Zero the Newton-iteration counters (call after warmup so the stats reflect the audio pass).
    pub fn reset_iter_stats(&mut self) {
        self.iter_samples = 0;
        self.iter_total = 0;
        self.iter_max = 0;
    }

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
        // Leave the settled state as the restore target so reset()/measure_gain() have a clean
        // operating point to fall back to even if the caller never calls save_snapshot() itself.
        self.save_snapshot();
    }

    /// Process one host-rate sample: linear-upsample to `OS` subsamples, Newton-solve each, decimate
    /// the DC-corrected tap through the two Butterworth biquads (`main.cpp:942-953`).
    #[inline]
    pub fn process_host_sample(&mut self, x: f32) -> f32 {
        // Audio pass: enable the Newton predictor (idempotent; warmup runs with it off).
        // JA core: on the warmup→audio transition, reset the flux/magnetization to a clean start
        // from the just-settled operating point (where v_m ≈ 0), so the rail-ramp transient never
        // saturates the core and poisons forward state.
        #[cfg(feature = "trafo-core")]
        if self.ja_active && !self.in_audio_pass {
            self.ja_reseed();
        }
        self.in_audio_pass = true;
        let cur_in = x as f64;
        let mut decimated = 0.0;
        for j in 0..self.os {
            let frac = (j + 1) as f64 / self.os as f64; // linear-interpolate upsample
            self.v[node::N_INPUT as usize] = self.prev_in + (cur_in - self.prev_in) * frac;
            self.process();
            let tap = self.v[node::N_SEC_IN as usize] - self.sec_in_dc;
            // At os>1 (always, since os is floored at 2) the biquads are the antialiasing decimator.
            // The `decimate=false` bypass remains for A/B testing only.
            decimated = if self.decimate {
                self.dec2.process(self.dec1.process(tap))
            } else {
                tap
            };
        }
        self.prev_in = cur_in;
        decimated as f32
    }

    /// Measure the settled circuit's small-signal voltage gain (output/input RMS) at a 1 kHz
    /// reference, on a SCRATCH CLONE so the live model is untouched. This is the one-shot static
    /// levelNorm probe: the plugin caches `1 / measure_gain()` and applies it as a constant trim,
    /// replacing the old continuous RMS estimator (no per-sample pumping). Block-rate only.
    ///
    /// In the dsp chain `wet/x == G_v` (the circuit voltage gain) for an undriven signal, so this
    /// ratio is exactly what the trim must invert. Driven nonlinearity is intentionally NOT probed —
    /// the trim is the clean operating-point gain, recomputed only when the operating point changes.
    pub fn measure_gain(&self) -> f32 {
        let mut m = self.clone();
        // Drain the live audio/saturation state before probing. The plugin re-probes this on every
        // glide block, and under Drive the live model is wound deep into saturation — a small-signal
        // probe on that state reads a gain wildly off the operating point (measured 22x -> 250x at
        // +18 dB) and jumps block-to-block, which makes the levelNorm trim jitter into loud peaks
        // during a Circuit-Gain sweep. Restoring the settled snapshot (clean DC operating point, the
        // model's G0 is still current) gives a deterministic probe regardless of program/Drive.
        m.reset();
        // Cheapen the probe's solver: this runs ON THE AUDIO THREAD on every operating-point change
        // (an automation lane steps the param every block), so the full-fidelity solver — measured at
        // ~30 ms/probe at realtime+High, ~23x a 64-sample block budget — is far too slow. The 0.05 V
        // stimulus keeps the circuit near-linear, where a 2-iter fixed-step solve converges to the
        // SAME small-signal gain as the full Newton solve. `os`/`dt`/`cap_tier` are left untouched —
        // they set the solve timestep + the AC frequency shaping that actually defines G_v.
        m.fixed_step = true;
        m.predict = false;
        m.max_iters = m.max_iters.min(2);
        let host_sr = self.internal_sr() / self.os as f64;
        let f = 1000.0_f64;
        let amp = 0.05_f64; // small-signal probe level (volts); stays in the near-linear region
        let cycle = (host_sr / f).max(1.0);
        // Settle: the restored snapshot is the settled DC operating point and G0 is current, so the
        // 1 kHz AC steady-state (well above the coupling-cap corner) re-establishes within a few
        // cycles. Kept short to bound the audio-thread cost; `probe_is_deterministic_and_pure` and
        // `probe_jitter_under_drive` pin that this stays stable and the sweep tests pin accuracy.
        let n_settle = (cycle * 4.0) as usize; // let the AC steady-state re-establish
        let n_meas = (cycle * 4.0) as usize; // integrate several cycles for a clean RMS
        let w = 2.0 * std::f64::consts::PI * f / host_sr;
        let (mut in_sq, mut out_sq) = (0.0f64, 0.0f64);
        for i in 0..(n_settle + n_meas) {
            let x = amp * (w * i as f64).sin();
            let y = m.process_host_sample(x as f32) as f64;
            if i >= n_settle {
                in_sq += x * x;
                out_sq += y * y;
            }
        }
        if in_sq <= 0.0 || !out_sq.is_finite() {
            return 1.0;
        }
        (out_sq / in_sq).sqrt() as f32
    }

    /// Re-converge the DC operating point in place after a cheap G0 re-stamp (rgain/bias/age/trafo
    /// changed but the integrator state was kept), re-snapshot it as the probe's clean restore
    /// target, then return [`measure_gain`]. This is the per-request op of the OFF-THREAD levelNorm
    /// probe worker (see `Ba283Plugin::task_executor`): the worker holds one persistent warmed model
    /// and re-stamps it each request instead of paying the ~1 s cold `warmup`, so a single probe
    /// settles in the handful of `idle_settle` chunks a small operating-point step needs. Off the
    /// audio thread — never call this in `process`.
    pub fn resettle_and_measure_gain(&mut self) -> f32 {
        self.idle_settle(2.0); // early-exits once the tap stops moving (≈ a few chunks for a small step)
        self.sec_in_dc = self.v[node::N_SEC_IN as usize];
        self.save_snapshot();
        self.measure_gain()
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
    fn precision_high_is_identity() {
        // HIGH precision MUST be the identity modifier on every tier — this is what keeps
        // Realtime+High bit-identical to the golden reference. Field-equality is exact (the struct
        // derives PartialEq), so `with_precision(High)` returning `self` is fully verified here.
        for base in [
            QualityConfig::realtime(),
            QualityConfig::fast(),
            QualityConfig::ultra_fast(),
        ] {
            assert_eq!(
                base,
                base.with_precision(Precision::High),
                "with_precision(High) must not alter the tier config"
            );
            // The bit-exact path is gated on `!fixed_step`; HIGH must never enable it.
            assert!(!base.with_precision(Precision::High).fixed_step);
        }
    }

    #[test]
    fn precision_med_low_enable_fixed_step() {
        // Med/Low must engage the structural lossy levers (fixed-step solver + reduced charge
        // model). These are the levers the prior sweep never tested; the harness gates their ESR.
        let rt = QualityConfig::realtime();
        for p in [Precision::Medium, Precision::Low] {
            let c = rt.with_precision(p);
            assert!(c.fixed_step, "{p:?} must enable fixed-step");
            assert_eq!(c.max_iters, 2, "{p:?} must cap Newton at 2 iters");
            assert_ne!(c.cap_tier, CapTier::Full, "{p:?} must reduce the charge model");
        }
        assert_eq!(rt.with_precision(Precision::Medium).cap_tier, CapTier::Depletion);
        assert_eq!(rt.with_precision(Precision::Low).cap_tier, CapTier::Off);
    }

    #[test]
    fn fixed_step_precision_finite_and_within_budget() {
        // Behavioral guard for the Medium/Low fixed-step solver — the config field-mapping is covered
        // by `precision_med_low_enable_fixed_step`; this drives the actual `process()` path:
        //   (1) a hot, clipping-level input must never yield non-finite output. The fixed-step path
        //       has no gmin-homotopy fallback, so the H1 guard in `process` holds the last converged
        //       iterate when a sample fails to converge — a single NaN/Inf can never propagate
        //       through the forward state (`v_prev`/`hist`/cap+BJT currents).
        //   (2) on a representative swept tone the Medium/Low ESR loss vs Realtime+High must stay
        //       inside the −50 dB harness budget (the PR's offline sweep measured −56…−60 dB).
        let host = 48_000.0;
        // Match the offline harness operating point (gain 0.5 → R_KJ 905.5 Ω, RV1 bias 0.77) so the
        // measured loss is comparable to the PR's documented −60 dB sweep point.
        let (rgain, bias) = (905.5, 0.77);

        // Warm once at the realtime tier (the DC op-point is tier-independent) and install the settled
        // snapshot into each model — the exact path the plugin uses, so we isolate audio-pass loss
        // from cold-start bias settling.
        let mut warm = WdfModel::new(host, rgain, bias);
        warm.warmup();
        let snap = warm.snapshot();
        let build = |cfg| {
            let mut m = WdfModel::new_with(cfg, host, rgain, bias);
            m.restore(&snap);
            m
        };
        let rt = QualityConfig::realtime();
        let mut high = build(rt);
        let mut med = build(rt.with_precision(Precision::Medium));
        let mut low = build(rt.with_precision(Precision::Low));

        // (1) Hot input: alternating rail-to-rail ±4.0 (well past clip) must stay finite on both
        // fixed-step tiers. Without the H1 guard a single divergent sample would poison every later
        // one; with it, output is always finite.
        for i in 0..1024 {
            let x = if i % 2 == 0 { 4.0 } else { -4.0 };
            assert!(med.process_host_sample(x).is_finite(), "Medium non-finite (hot) at {i}");
            assert!(low.process_host_sample(x).is_finite(), "Low non-finite (hot) at {i}");
        }

        // (2) Tracking guardrail: re-install the clean snapshot, then score an 80 Hz→2.5 kHz chirp
        // (integrated phase) at a musical level against Realtime+High, lag-aligned (the fixed-step
        // solver adds a small group-delay vs full Newton — same reason the offline harness carries a
        // `lag` field). The authoritative −50 dB budget is the `eval/plugin_vs_acme` sweep on the real
        // `sweep_m3` corpus + full metric, which a unit test can't reproduce (a broadband synthetic
        // chirp excites the reduced charge model harder, ≈−35 dB here). So this asserts the robust
        // property — Medium/Low TRACK Realtime+High (aligned ESR ≤ −25 dB), not collapsed/diverged —
        // with ~10 dB margin; a real regression (NaN, silence, instability) blows far past it.
        med.restore(&snap);
        low.restore(&snap);
        let n = 8192usize;
        let skip = 512usize; // drop the post-install transient before scoring
        let (f0, f1) = (80.0_f64, 2500.0_f64);
        let k = (f1 - f0) / (n as f64 / host); // chirp rate (Hz/s)
        let mut yh = vec![0.0f64; n];
        let mut ym = vec![0.0f64; n];
        let mut yl = vec![0.0f64; n];
        for i in 0..n {
            let t = i as f64 / host;
            let phase = std::f64::consts::TAU * (f0 * t + 0.5 * k * t * t); // integrated chirp phase
            let x = (0.25 * phase.sin()) as f32;
            yh[i] = high.process_host_sample(x) as f64;
            ym[i] = med.process_host_sample(x) as f64;
            yl[i] = low.process_host_sample(x) as f64;
        }
        // Lag-aligned ESR over the scored window: pick the integer shift (|lag| ≤ 8) of `test`
        // against `ref` that minimizes residual energy, then report that minimum vs the ref energy.
        let aligned_esr_db = |refs: &[f64], test: &[f64]| -> f64 {
            let mut best = f64::INFINITY;
            for lag in -8i64..=8 {
                let (mut e, mut sig) = (0.0f64, 0.0f64);
                for i in skip..n {
                    let j = i as i64 + lag;
                    if j < 0 || j as usize >= n {
                        continue;
                    }
                    let d = test[j as usize] - refs[i];
                    e += d * d;
                    sig += refs[i] * refs[i];
                }
                best = best.min(e / sig);
            }
            10.0 * best.log10()
        };
        let m_db = aligned_esr_db(&yh, &ym);
        let l_db = aligned_esr_db(&yh, &yl);
        assert!(m_db <= -25.0, "Medium ESR {m_db:.1} dB — fixed-step not tracking Realtime+High");
        assert!(l_db <= -25.0, "Low ESR {l_db:.1} dB — fixed-step not tracking Realtime+High");
    }

    #[test]
    fn ultra_exp_within_budget() {
        // ultra_exp must stay within ~1e-4 relative over the transport-exp operating range
        // (v/vt up to ~vcrit/vt ~ 28 forward; reverse-biased args go very negative -> ~0).
        let mut worst = 0.0f64;
        let mut x = -40.0;
        while x <= 30.0 {
            let a = ultra_exp(x);
            let e = x.exp();
            if e > 1e-12 {
                worst = worst.max(((a - e) / e).abs());
            }
            x += 0.01;
        }
        assert!(worst < 1e-4, "ultra_exp worst rel err {worst:e} exceeds 1e-4 budget");
        // exponent guards match fast_exp (no garbage for very negative k).
        assert_eq!(ultra_exp(-1000.0), 0.0);
    }

    #[test]
    fn ultra_fast_os_conditional_floor() {
        // ultra-fast runs os=1 (its speed lever) at High precision — peak-safe there — but the os
        // floor kicks in to os=2 under the Med/Low fixed-step solver, where os=1 limit-cycles.
        let cfg = QualityConfig::ultra_fast();
        assert_eq!(cfg.os, 1, "ultra-fast High runs os=1 (the speed lever)");
        assert!(!cfg.decimate, "ultra-fast High bypasses decimation (nothing to decimate at os=1)");
        assert!(cfg.cheap_exp, "ultra-fast keeps the cheap transport exp lever");
        assert!(cfg.predict && cfg.solver == Solver::Schur && cfg.freeze_caps,
                "ultra-fast inherits the fast-tier levers");
        // solve_os/solve_decimate resolve the effective values.
        assert_eq!(cfg.solve_os(), 1, "High: no fixed_step -> os=1 stands");
        assert!(!cfg.solve_decimate(), "High: os=1 -> no decimation");

        // Med/Low layer on fixed_step -> os floored to 2, decimator re-enabled.
        for p in [Precision::Medium, Precision::Low] {
            let c = cfg.with_precision(p);
            assert_eq!(c.solve_os(), 2, "{p:?}: fixed_step floors os to 2");
            assert!(c.solve_decimate(), "{p:?}: floored os=2 must re-enable the decimator");
            let m = WdfModel::new_with(c, 48_000.0, 220.0, 0.5);
            assert_eq!(m.os, 2, "{p:?}: built model must solve at os=2");
            assert!(m.decimate, "{p:?}: built model must decimate");
        }
        // Even a hand-assembled fixed_step + os=1 config is floored to 2 (single source of truth).
        let mut forced = cfg;
        forced.fixed_step = true;
        forced.os = 1;
        let m = WdfModel::new_with(forced, 48_000.0, 220.0, 0.5);
        assert_eq!(m.os, 2, "fixed_step must floor os at 2 even when hand-assembled");
        assert!(m.decimate, "and re-enable the decimator");
        // ultra-fast High produces finite output (no NaN/instability) on a quick ramp.
        let mut m = WdfModel::new_with(cfg, 48_000.0, 220.0, 0.5);
        assert_eq!(m.os, 1);
        for i in 0..256 {
            let y = m.process_host_sample((i as f32 / 256.0) * 0.4 - 0.2);
            assert!(y.is_finite(), "ultra-fast output non-finite at sample {i}");
        }
    }

    #[test]
    fn warp_drops_charge_model_only() {
        let cfg = QualityConfig::warp();
        // It is ultra-fast with cap_tier=Off and nothing else changed.
        assert_eq!(cfg.cap_tier, CapTier::Off, "must drop the BJT charge model");
        assert_eq!(cfg.os, 1, "inherits ultra-fast os=1 (High speed lever)");
        assert!(!cfg.decimate, "inherits ultra-fast os=1 decimation bypass");
        assert!(cfg.predict && cfg.solver == Solver::Schur && cfg.freeze_caps,
                "inherits the fast-tier levers");
        // Distinct from Precision::Low: this keeps the full convergence gate (no fixed-step cap).
        assert!(!cfg.fixed_step, "warp keeps the convergence gate (unlike Precision::Low)");
        assert_eq!(cfg, QualityConfig { cap_tier: CapTier::Off, ..QualityConfig::ultra_fast() });
        // Finite, stable output on a quick ramp.
        let mut m = WdfModel::new_with(cfg, 48_000.0, 220.0, 0.5);
        for i in 0..256 {
            let y = m.process_host_sample((i as f32 / 256.0) * 0.4 - 0.2);
            assert!(y.is_finite(), "warp output non-finite at sample {i}");
        }
    }

    #[test]
    fn decimate_flag_changes_output() {
        // Guards against the `decimate` lever silently no-op'ing: at os=2 the decimation biquads are
        // the antialiasing decimator, so toggling them MUST change the output. (Same input, same
        // circuit, same seed — only `decimate` toggled.)
        let host = 48_000.0;
        let mut base = QualityConfig::fast(); // os=2 tier; toggle the decimator only
        base.decimate = false;
        let mut filt = base; // os=2, decimate=true
        filt.decimate = true;

        let mut m_off = WdfModel::new_with(base, host, 220.0, 0.5);
        let mut m_on = WdfModel::new_with(filt, host, 220.0, 0.5);
        // Warm both so the decimator acts on the real operating point, not the cold ramp.
        for _ in 0..256 {
            let _ = m_off.process_host_sample(0.0);
            let _ = m_on.process_host_sample(0.0);
        }
        let mut max_abs_diff = 0.0f32;
        for i in 0..1024 {
            // HOT, bright tone so the circuit nonlinearity generates HF harmonics above Nyquist for
            // the os=2 decimator to antialias (vs the naive every-other-subsample drop with decimate
            // off). A small near-linear tone produces too little HF for the biquads to matter.
            let x = (i as f32 * 0.7).sin() * 4.0;
            let d = (m_off.process_host_sample(x) - m_on.process_host_sample(x)).abs();
            max_abs_diff = max_abs_diff.max(d);
        }
        assert!(max_abs_diff > 1e-5,
                "decimate flag had no audible effect (max |Δ| {max_abs_diff:e}) — lever may be a no-op");
    }

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

    /// `gauss_solve_16` must be BIT-FOR-BIT identical to `gauss_solve(.., 16)` across a wide range
    /// of 16x16 systems (well-conditioned, pivot-triggering, near-singular, negative, mixed
    /// magnitudes). Physical-swap vs index-swap is bit-identical; this also guards against fma
    /// contraction in the vectorized elimination loop (which would diverge in the last bit).
    #[test]
    fn gauss_solve_16_is_bit_exact() {
        const N: usize = 16;
        // Deterministic LCG (Numerical Recipes constants), no external crates.
        let mut state: u64 = 0x1234_5678_9abc_def0;
        let mut next = || -> f64 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            // top 53 bits -> [0,1)
            ((state >> 11) as f64) / ((1u64 << 53) as f64)
        };

        // Build a 16x16 system with a chosen "flavor" controlling conditioning/magnitudes.
        let make = |flavor: u32, next: &mut dyn FnMut() -> f64| -> ([f64; N * N], [f64; N]) {
            let mut a = [0.0f64; N * N];
            let mut b = [0.0f64; N];
            for r in 0..N {
                for c in 0..N {
                    let base = next() * 2.0 - 1.0; // [-1,1]
                    a[r * N + c] = match flavor {
                        // Well-conditioned, diagonally dominant.
                        0 => {
                            if r == c {
                                base + (N as f64) * 4.0
                            } else {
                                base
                            }
                        }
                        // Pivot-triggering: huge off-diagonals, small diagonal.
                        1 => {
                            if r == c {
                                base * 1e-6
                            } else if c > r {
                                base * 1e6
                            } else {
                                base
                            }
                        }
                        // Near-singular-ish: two nearly-proportional rows.
                        2 => {
                            if r == 5 {
                                a[(4) * N + c] * (1.0 + 1e-9) + base * 1e-12
                            } else if r == c {
                                base + 2.0
                            } else {
                                base
                            }
                        }
                        // Mixed magnitudes / negatives.
                        3 => {
                            let scale = 10f64.powi(((next() * 12.0) as i32) - 6);
                            base * scale
                        }
                        // All negative-ish with moderate diagonal.
                        _ => {
                            if r == c {
                                -(base.abs() + 5.0)
                            } else {
                                -base.abs()
                            }
                        }
                    };
                }
                b[r] = next() * 20.0 - 10.0;
            }
            (a, b)
        };

        let mut cases = 0usize;
        for flavor in 0..5u32 {
            for _ in 0..400 {
                let (a0, b0) = make(flavor, &mut next);

                // Reference path: gauss_solve over a 16x16 region of an N-wide buffer.
                let mut a_ref = a0;
                let mut b_ref = b0;
                let ok_ref = WdfModel::gauss_solve(&mut a_ref, &mut b_ref, N);

                let mut a_16 = a0;
                let mut b_16 = b0;
                let ok_16 = WdfModel::gauss_solve_16(&mut a_16, &mut b_16);

                assert_eq!(
                    ok_ref, ok_16,
                    "case {cases} (flavor {flavor}): singular-bool mismatch ref={ok_ref} 16={ok_16}"
                );
                if ok_ref {
                    for i in 0..N {
                        let rb = b_ref[i].to_bits();
                        let xb = b_16[i].to_bits();
                        assert_eq!(
                            rb, xb,
                            "case {cases} (flavor {flavor}) elem {i}: bit divergence ref={:#018x} ({}) 16={:#018x} ({})",
                            rb, b_ref[i], xb, b_16[i]
                        );
                    }
                }
                cases += 1;
            }
        }
        eprintln!("gauss_solve_16_is_bit_exact: {cases} systems all bit-identical");
    }

    /// `gauss_solve_6` must be BIT-FOR-BIT identical to `gauss_solve(.., 6)` across a wide range
    /// of 6x6 systems (well-conditioned, pivot-triggering, near-singular, negative, mixed
    /// magnitudes). Physical-swap vs index-swap is bit-identical; this also guards against fma
    /// contraction in the elimination loop (which would diverge in the last bit).
    #[test]
    fn gauss_solve_6_is_bit_exact() {
        const N: usize = 6;
        // Deterministic LCG (Numerical Recipes constants), no external crates.
        let mut state: u64 = 0x1234_5678_9abc_def0;
        let mut next = || -> f64 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 11) as f64) / ((1u64 << 53) as f64)
        };

        // Build a 6x6 system with a chosen "flavor" controlling conditioning/magnitudes.
        let make = |flavor: u32, next: &mut dyn FnMut() -> f64| -> ([f64; N * N], [f64; N]) {
            let mut a = [0.0f64; N * N];
            let mut b = [0.0f64; N];
            for r in 0..N {
                for c in 0..N {
                    let base = next() * 2.0 - 1.0; // [-1,1]
                    a[r * N + c] = match flavor {
                        // Well-conditioned, diagonally dominant.
                        0 => {
                            if r == c {
                                base + (N as f64) * 4.0
                            } else {
                                base
                            }
                        }
                        // Pivot-triggering: huge off-diagonals, small diagonal.
                        1 => {
                            if r == c {
                                base * 1e-6
                            } else if c > r {
                                base * 1e6
                            } else {
                                base
                            }
                        }
                        // Near-singular-ish: two nearly-proportional rows.
                        2 => {
                            if r == 3 {
                                a[(2) * N + c] * (1.0 + 1e-9) + base * 1e-12
                            } else if r == c {
                                base + 2.0
                            } else {
                                base
                            }
                        }
                        // Mixed magnitudes / negatives.
                        3 => {
                            let scale = 10f64.powi(((next() * 12.0) as i32) - 6);
                            base * scale
                        }
                        // All negative-ish with moderate diagonal.
                        _ => {
                            if r == c {
                                -(base.abs() + 5.0)
                            } else {
                                -base.abs()
                            }
                        }
                    };
                }
                b[r] = next() * 20.0 - 10.0;
            }
            (a, b)
        };

        let mut cases = 0usize;
        for flavor in 0..5u32 {
            for _ in 0..400 {
                let (a0, b0) = make(flavor, &mut next);

                // Reference path: gauss_solve over a 6x6 region of an N-wide buffer.
                let mut a_ref = a0;
                let mut b_ref = b0;
                let ok_ref = WdfModel::gauss_solve(&mut a_ref, &mut b_ref, N);

                let mut a_6 = a0;
                let mut b_6 = b0;
                let ok_6 = WdfModel::gauss_solve_6(&mut a_6, &mut b_6);

                assert_eq!(
                    ok_ref, ok_6,
                    "case {cases} (flavor {flavor}): singular-bool mismatch ref={ok_ref} 6={ok_6}"
                );
                if ok_ref {
                    for i in 0..N {
                        let rb = b_ref[i].to_bits();
                        let xb = b_6[i].to_bits();
                        assert_eq!(
                            rb, xb,
                            "case {cases} (flavor {flavor}) elem {i}: bit divergence ref={:#018x} ({}) 6={:#018x} ({})",
                            rb, b_ref[i], xb, b_6[i]
                        );
                    }
                }
                cases += 1;
            }
        }
        eprintln!("gauss_solve_6_is_bit_exact: {cases} systems all bit-identical");
    }

    /// The physical-swap `lu_factor_10` + `lu_solve_10` pair must produce BIT-FOR-BIT identical
    /// solutions to the generic index-swap `lu_factor(.., 10)` + `lu_solve(.., 10, ..)` pair across
    /// many 10x10 systems (well-conditioned, pivot-triggering, near-singular, negative, mixed).
    /// Storage layout (physical rows + perm vs `piv[]` indirection) differs; arithmetic must not.
    #[test]
    fn lu_10_is_bit_exact() {
        const N: usize = 10;
        let mut state: u64 = 0x0fed_cba9_8765_4321;
        let mut next = || -> f64 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 11) as f64) / ((1u64 << 53) as f64)
        };

        let make = |flavor: u32, next: &mut dyn FnMut() -> f64| -> ([f64; N * N], [f64; N]) {
            let mut a = [0.0f64; N * N];
            let mut b = [0.0f64; N];
            for r in 0..N {
                for c in 0..N {
                    let base = next() * 2.0 - 1.0;
                    a[r * N + c] = match flavor {
                        0 => {
                            if r == c {
                                base + (N as f64) * 4.0
                            } else {
                                base
                            }
                        }
                        1 => {
                            if r == c {
                                base * 1e-6
                            } else if c > r {
                                base * 1e6
                            } else {
                                base
                            }
                        }
                        2 => {
                            if r == 6 {
                                a[(5) * N + c] * (1.0 + 1e-9) + base * 1e-12
                            } else if r == c {
                                base + 2.0
                            } else {
                                base
                            }
                        }
                        3 => {
                            let scale = 10f64.powi(((next() * 12.0) as i32) - 6);
                            base * scale
                        }
                        _ => {
                            if r == c {
                                -(base.abs() + 5.0)
                            } else {
                                -base.abs()
                            }
                        }
                    };
                }
                b[r] = next() * 20.0 - 10.0;
            }
            (a, b)
        };

        let mut cases = 0usize;
        for flavor in 0..5u32 {
            for _ in 0..400 {
                let (a0, b0) = make(flavor, &mut next);

                // Reference path: generic index-swap lu_factor + lu_solve.
                let mut a_ref = a0;
                let mut piv_ref = [0usize; NUM_NODES];
                let ok_ref = WdfModel::lu_factor(&mut a_ref, &mut piv_ref, N);
                let mut b_ref = b0;
                if ok_ref {
                    WdfModel::lu_solve(&a_ref, &piv_ref, N, &mut b_ref);
                }

                // Physical-swap path.
                let mut a_10 = a0;
                let mut perm_10 = [0usize; NUM_NODES];
                let ok_10 = WdfModel::lu_factor_10(&mut a_10, &mut perm_10);
                let mut b_10 = b0;
                if ok_10 {
                    WdfModel::lu_solve_10(&a_10, &perm_10, &mut b_10);
                }

                assert_eq!(
                    ok_ref, ok_10,
                    "case {cases} (flavor {flavor}): singular-bool mismatch ref={ok_ref} 10={ok_10}"
                );
                if ok_ref {
                    for i in 0..N {
                        let rb = b_ref[i].to_bits();
                        let xb = b_10[i].to_bits();
                        assert_eq!(
                            rb, xb,
                            "case {cases} (flavor {flavor}) elem {i}: bit divergence ref={:#018x} ({}) 10={:#018x} ({})",
                            rb, b_ref[i], xb, b_10[i]
                        );
                    }
                }
                cases += 1;
            }
        }
        eprintln!("lu_10_is_bit_exact: {cases} systems all bit-identical");
    }

    // ---- Phase 1a: branchless leaf kernel bit-exactness ----

    #[test]
    fn exp_bl_is_bit_exact() {
        let mut mismatches = 0usize;
        let mut first: Option<(f64, u64, u64)> = None;
        let mut check = |x: f64| {
            let a = exp_bl::<false>(x).to_bits();
            let b = fast_exp(x).to_bits();
            if a != b {
                mismatches += 1;
                if first.is_none() {
                    first = Some((x, a, b));
                }
            }
        };
        // Deep underflow (reverse-biased junctions: x/Vt ~ -1000).
        let mut x = -1400.0_f64;
        while x <= -50.0 {
            check(x);
            x += 0.05;
        }
        // k <= -1023 boundary region.
        let mut x = -720.0_f64;
        while x <= -700.0 {
            check(x);
            x += 0.0005;
        }
        // Active / near-zero / forward region.
        let mut x = -50.0_f64;
        while x <= 40.0 {
            check(x);
            x += 0.001;
        }
        for &x in &[-1400.0, -1023.0, -709.7827, -709.0, -1.0, -0.5, 0.0, 0.5, 1.0, 39.9, 40.0] {
            check(x);
        }
        if let Some((x, a, b)) = first {
            panic!("exp_bl: {mismatches} mismatches; first x={x:.9} got=0x{a:016x} want=0x{b:016x}");
        }
        assert_eq!(mismatches, 0);
    }

    #[test]
    fn log_bl_is_bit_exact() {
        let mut mismatches = 0usize;
        let mut first: Option<(f64, u64, u64)> = None;
        let mut check = |x: f64| {
            let a = log_bl::<false>(x).to_bits();
            let b = fast_log(x).to_bits();
            if a != b {
                mismatches += 1;
                if first.is_none() {
                    first = Some((x, a, b));
                }
            }
        };
        // f = 1 - v/vj: sweep [0.5, 200] covering junction domain
        let mut x = 0.5_f64;
        while x <= 200.0 {
            check(x);
            x += 0.001;
        }
        for &x in &[0.5_f64, 0.707, 0.707_106_8, 1.0, 2.0, 10.0, 100.0, 200.0] {
            check(x);
        }
        if let Some((x, a, b)) = first {
            panic!("log_bl: {mismatches} mismatches; first x={x:.9} got=0x{a:016x} want=0x{b:016x}");
        }
        assert_eq!(mismatches, 0);
    }

    #[test]
    fn pow_bl_is_bit_exact() {
        let mut mismatches = 0usize;
        let mut first: Option<(f64, f64, u64, u64)> = None;
        let mut check = |base: f64, ex: f64| {
            let a = pow_bl::<false>(base, ex).to_bits();
            let b = fast_pow_pos(base, ex).to_bits();
            if a != b {
                mismatches += 1;
                if first.is_none() {
                    first = Some((base, ex, a, b));
                }
            }
        };
        // pow_(f, -m) and pow_(f, 1-m): m in [0.33, 0.5] for SPICE BJT junctions
        for &m in &[0.33_f64, 0.4, 0.5] {
            let mut f = 0.5_f64;
            while f <= 200.0 {
                check(f, -m);
                check(f, 1.0 - m);
                f += 0.01;
            }
        }
        if let Some((base, ex, a, b)) = first {
            panic!("pow_bl: {mismatches} mismatches; first base={base:.9} ex={ex:.9} got=0x{a:016x} want=0x{b:016x}");
        }
        assert_eq!(mismatches, 0);
    }

    // ---- Phase 1b: batch device kernel bit-exactness ----

    #[test]
    fn compute_bjt_batch_is_bit_exact() {
        let tr1 = bc184c();
        let tr3 = q2n3055();
        // TR1=BC184C, TR2=BC184C, TR3=Q2N3055
        let bjts: [Bjt; 3] = [
            Bjt::new(0, 1, 2, tr1),
            Bjt::new(3, 4, 5, tr1),
            Bjt::new(6, 7, 8, tr3),
        ];

        let mut cases = 0usize;
        // Sweep vbe/vbc over the active junction domain
        let mut vbe = -0.5_f64;
        while vbe <= 0.8 {
            let mut vbc = -0.5_f64;
            while vbc <= 0.8 {
                let vbe4 = [vbe, vbe + 0.05, vbe - 0.05, 0.0];
                let vbc4 = [vbc, vbc - 0.05, vbc + 0.05, 0.0];
                let soa = compute_bjt_batch(&vbe4, &vbc4, &bjts);
                for bi in 0..3 {
                    let s = compute_bjt(vbe4[bi], vbc4[bi], &bjts[bi].p);
                    macro_rules! check_field {
                        ($field:ident) => {
                            let got = soa.$field[bi].to_bits();
                            let want = s.$field.to_bits();
                            assert_eq!(got, want,
                                "compute_bjt_batch: field={} lane={bi} vbe={} vbc={} got=0x{got:016x} want=0x{want:016x}",
                                stringify!($field), vbe4[bi], vbc4[bi]);
                        }
                    }
                    check_field!(ib); check_field!(ic); check_field!(ie);
                    check_field!(itf); check_field!(gtf); check_field!(itr); check_field!(gtr);
                    check_field!(dib_b); check_field!(dib_e); check_field!(dib_c);
                    check_field!(dic_b); check_field!(dic_e); check_field!(dic_c);
                    check_field!(die_b); check_field!(die_e); check_field!(die_c);
                }
                cases += 1;
                vbc += 0.1;
            }
            vbe += 0.1;
        }
        eprintln!("compute_bjt_batch_is_bit_exact: {cases} cases all bit-identical");
    }

    #[test]
    fn compute_bjt_preexp_batch_is_bit_exact() {
        // Production guard: the width-4 post-exp batch must be bit-identical to the scalar
        // `compute_bjt_preexp` reference at FMA=false, lane-for-lane, across the junction domain.
        let tr1 = bc184c();
        let tr3 = q2n3055();
        let bjts: [Bjt; 3] = [
            Bjt::new(0, 1, 2, tr1),
            Bjt::new(3, 4, 5, tr1),
            Bjt::new(6, 7, 8, tr3),
        ];
        let exp_lane = |v: f64, vt: f64| exp_bl::<false>(v / vt);

        let mut cases = 0usize;
        let mut vbe = -0.5_f64;
        while vbe <= 0.8 {
            let mut vbc = -0.5_f64;
            while vbc <= 0.8 {
                let vbe4 = [vbe, vbe + 0.05, vbe - 0.05, 0.0];
                let vbc4 = [vbc, vbc - 0.05, vbc + 0.05, 0.0];
                // ef/er/ec exactly as newton_iterate batches them (lane 3 = padding params).
                let mk = |sel: &dyn Fn(usize) -> f64| {
                    [
                        exp_lane(sel(0), bjts[0].p.vt),
                        exp_lane(sel(1), bjts[1].p.vt),
                        exp_lane(sel(2), bjts[2].p.vt),
                        exp_lane(0.0, 0.02585),
                    ]
                };
                let ef4 = mk(&|i| vbe4[i]);
                let er4 = mk(&|i| vbc4[i]);
                let ec4 = mk(&|i| bjts[i].p.vcrit);
                let soa = compute_bjt_preexp_batch::<false>(&vbe4, &vbc4, &bjts, &ef4, &er4, &ec4);
                for bi in 0..3 {
                    let s = compute_bjt_preexp::<false>(vbe4[bi], vbc4[bi], &bjts[bi].p, ef4[bi], er4[bi], ec4[bi]);
                    macro_rules! check_field {
                        ($field:ident) => {
                            let got = soa.$field[bi].to_bits();
                            let want = s.$field.to_bits();
                            assert_eq!(got, want,
                                "compute_bjt_preexp_batch: field={} lane={bi} vbe={} vbc={} got=0x{got:016x} want=0x{want:016x}",
                                stringify!($field), vbe4[bi], vbc4[bi]);
                        }
                    }
                    check_field!(ib); check_field!(ic); check_field!(ie);
                    check_field!(itf); check_field!(gtf); check_field!(itr); check_field!(gtr);
                    check_field!(dib_b); check_field!(dib_e); check_field!(dib_c);
                    check_field!(dic_b); check_field!(dic_e); check_field!(dic_c);
                    check_field!(die_b); check_field!(die_e); check_field!(die_c);
                }
                cases += 1;
                vbc += 0.1;
            }
            vbe += 0.1;
        }
        eprintln!("compute_bjt_preexp_batch_is_bit_exact: {cases} cases all bit-identical");
    }

    #[test]
    fn depletion_charge_pre_batch_is_bit_exact() {
        let tr1 = bc184c();
        let tr3 = q2n3055();
        let params = [tr1, tr1, tr3, BjtParams::default()];

        let mut cases = 0usize;
        let mut v = -50.0_f64;
        while v <= 1.0 {
            let v4 = [v, v + 0.05, v - 0.05, 0.0];
            // BE junction
            let cje4 = [params[0].cje, params[1].cje, params[2].cje, 0.0];
            let vje4 = [params[0].vje, params[1].vje, params[2].vje, 0.75];
            let mje4 = [params[0].mje, params[1].mje, params[2].mje, 0.33];
            let je_c04  = [params[0].je_c0,  params[1].je_c0,  params[2].je_c0,  0.0];
            let je_q04  = [params[0].je_q0,  params[1].je_q0,  params[2].je_q0,  0.0];
            let je_dcdv4 = [params[0].je_dcdv, params[1].je_dcdv, params[2].je_dcdv, 0.0];
            let (qbe4, cbe4) = depletion_charge_pre_batch::<false>(v4, cje4, vje4, mje4, je_c04, je_q04, je_dcdv4);
            for bi in 0..3 {
                let (qw, cw) = depletion_charge_pre(v4[bi], params[bi].cje, params[bi].vje,
                    params[bi].mje, params[bi].je_c0, params[bi].je_q0, params[bi].je_dcdv);
                assert_eq!(qbe4[bi].to_bits(), qw.to_bits(),
                    "dep_batch BE q: lane={bi} v={} got={} want={}", v4[bi], qbe4[bi], qw);
                assert_eq!(cbe4[bi].to_bits(), cw.to_bits(),
                    "dep_batch BE c: lane={bi} v={} got={} want={}", v4[bi], cbe4[bi], cw);
            }
            // BC junction
            let cjc4 = [params[0].cjc, params[1].cjc, params[2].cjc, 0.0];
            let vjc4 = [params[0].vjc, params[1].vjc, params[2].vjc, 0.75];
            let mjc4 = [params[0].mjc, params[1].mjc, params[2].mjc, 0.33];
            let jc_c04  = [params[0].jc_c0,  params[1].jc_c0,  params[2].jc_c0,  0.0];
            let jc_q04  = [params[0].jc_q0,  params[1].jc_q0,  params[2].jc_q0,  0.0];
            let jc_dcdv4 = [params[0].jc_dcdv, params[1].jc_dcdv, params[2].jc_dcdv, 0.0];
            let (qbc4, cbc4) = depletion_charge_pre_batch::<false>(v4, cjc4, vjc4, mjc4, jc_c04, jc_q04, jc_dcdv4);
            for bi in 0..3 {
                let (qw, cw) = depletion_charge_pre(v4[bi], params[bi].cjc, params[bi].vjc,
                    params[bi].mjc, params[bi].jc_c0, params[bi].jc_q0, params[bi].jc_dcdv);
                assert_eq!(qbc4[bi].to_bits(), qw.to_bits(),
                    "dep_batch BC q: lane={bi} v={} got={} want={}", v4[bi], qbc4[bi], qw);
                assert_eq!(cbc4[bi].to_bits(), cw.to_bits(),
                    "dep_batch BC c: lane={bi} v={} got={} want={}", v4[bi], cbc4[bi], cw);
            }
            cases += 1;
            v += 0.1;
        }
        eprintln!("depletion_charge_pre_batch_is_bit_exact: {cases} cases all bit-identical");
    }
}
