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
pub(in crate::wdf) fn fma<const FMA: bool>(a: f64, b: f64, c: f64) -> f64 {
    if FMA { a.mul_add(b, c) } else { a * b + c }
}

/// Branchless 9th-order Taylor `exp`, bit-exact with [`fast_exp`] at `FMA=false`.
/// Branches → if/else selects so SLP can fuse 4 lanes into AVX2 ymm ops.
#[inline]
pub(in crate::wdf) fn exp_bl<const FMA: bool>(x: f64) -> f64 {
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
pub(in crate::wdf) fn log_bl<const FMA: bool>(x: f64) -> f64 {
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
pub(in crate::wdf) fn pow_bl<const FMA: bool>(base: f64, ex: f64) -> f64 {
    exp_bl::<FMA>(ex * log_bl::<FMA>(base))
}

/// Width-4 `exp_bl(v[i]*inv_vt[i])`, generic over FMA — wraps the per-tier instantiation so
/// `newton_iterate` can pick FMA on/off with one branch instead of four `::<F>` turbofish sites.
#[inline(always)]
pub(in crate::wdf) fn exp4<const FMA: bool>(v: &[f64; 4], inv_vt: &[f64; 4]) -> [f64; 4] {
    [
        exp_bl::<FMA>(v[0] * inv_vt[0]), exp_bl::<FMA>(v[1] * inv_vt[1]),
        exp_bl::<FMA>(v[2] * inv_vt[2]), exp_bl::<FMA>(v[3] * inv_vt[3]),
    ]
}


#[cfg(test)]
mod tests {
    use super::*;

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

}
