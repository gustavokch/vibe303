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
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
pub struct Res {
    pub a: i32,
    pub b: i32,
    pub g: f64,
}

/// Capacitor with trapezoidal companion: `geq = 2C/dt`, current `i[n] = geq*(dv - dv_prev) -
/// i_prev` (`main.cpp:249-250`).
#[derive(Clone, Copy, Debug)]
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
