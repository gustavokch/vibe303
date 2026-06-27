//! Fast transcendentals for the nodal Newton solver (F3.1).
//!
//! Range-reduced `exp`/`log` ported from `~/Git/ba283_wdf/main.cpp`
//! (`fast_exp`/`fast_log`). The diode stamp calls `exp` per node per Newton
//! iteration, so this is the hot transcendental. `|rel err| < ~1e-11` over the
//! domains used here — far below the Newton tol — so converged node voltages
//! are identical to the `std::` path to ~12 digits.
//!
//! Uses `f64::to_bits`/`from_bits` (Rust's strict-aliasing-safe analog of the
//! C++ `memcpy` round-trip).

/// `exp(x)` via range reduction `x = k·ln2 + r`, Taylor on `r`, `2^k` by direct
/// IEEE-754 exponent assembly.
#[inline]
pub fn fast_exp(x: f64) -> f64 {
    const LN2: f64 = 0.6931471805599453;
    const INV_LN2: f64 = 1.4426950408889634;
    let kf = (x * INV_LN2 + 0.5).floor();
    let k = kf as i64;
    let r = x - kf * LN2; // |r| <= ln2/2 ~ 0.3466
                          // Taylor to r^9 -> remainder ~ r^10/10! < 1e-11 on this range.
    let p = 1.0
        + r * (1.0
            + r * (1.0 / 2.0
                + r * (1.0 / 6.0
                    + r * (1.0 / 24.0
                        + r * (1.0 / 120.0
                            + r * (1.0 / 720.0
                                + r * (1.0 / 5040.0
                                    + r * (1.0 / 40320.0 + r * (1.0 / 362880.0)))))))));
    // 2^k via the IEEE-754 exponent field. MUST guard negative k: reverse-biased
    // junctions give v/Vt ~ -1000 -> k ~ -1339; without the guard, (k+1023) goes
    // negative and the bit pattern wraps to garbage, poisoning the Jacobian.
    if k <= -1023 {
        return 0.0; // underflow: true exp is subnormal/negligible (<1e-308)
    }
    if k >= 1024 {
        return p * f64::INFINITY; // never in domain
    }
    let u = ((k + 1023) as u64) << 52;
    p * f64::from_bits(u)
}

/// `ln(x)` via IEEE-754 exponent decompose `x = m·2^e`, `log(m) = 2·atanh(s)`.
#[inline]
pub fn fast_log(x: f64) -> f64 {
    const LN2: f64 = 0.6931471805599453;
    let bits = x.to_bits();
    let mut e = ((bits >> 52) & 0x7ff) as i64 - 1022; // m in [0.5,1)
    let m_bits = (bits & 0x000f_ffff_ffff_ffff) | 0x3fe0_0000_0000_0000;
    let mut m = f64::from_bits(m_bits);
    if m < 0.7071067811865476 {
        m *= 2.0; // center: m in [~0.707,~1.414)
        e -= 1;
    }
    let f = m - 1.0;
    let s = f / (2.0 + f);
    let s2 = s * s; // log(m) = 2*atanh(s)
    let poly = 2.0
        * s
        * (1.0
            + s2 * (1.0 / 3.0
                + s2 * (1.0 / 5.0 + s2 * (1.0 / 7.0 + s2 * (1.0 / 9.0 + s2 / 11.0)))));
    poly + e as f64 * LN2
}

/// Positive-base power via `exp(ex·log(base))`; only called with `base > 0`.
#[inline]
pub fn fast_pow_pos(base: f64, ex: f64) -> f64 {
    fast_exp(ex * fast_log(base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_exp_matches_std_over_junction_domain() {
        for i in -2000..=200 {
            // v/Vt ranges deeply negative (reverse bias) to ~+5
            let x = i as f64 * 0.05;
            let (a, b) = (fast_exp(x), x.exp());
            let rel = (a - b).abs() / b.max(1e-300);
            assert!(rel < 1e-9 || b < 1e-300, "exp({x}) rel err {rel}");
        }
        assert_eq!(fast_exp(-800.0), 0.0); // deep reverse bias underflows cleanly, no NaN
    }

    #[test]
    fn fast_log_matches_std() {
        for i in 1..100_000 {
            let x = i as f64 * 1e-3;
            assert!((fast_log(x) - x.ln()).abs() < 1e-9, "log({x})");
        }
    }

    #[test]
    fn fast_pow_pos_matches_std() {
        for &(b, e) in &[(0.5, -0.333), (1.2, 0.667), (0.75, 1.5), (1.0, -2.0)] {
            assert!(
                (fast_pow_pos(b, e) - b.powf(e)).abs() < 1e-9,
                "pow({b},{e})"
            );
        }
    }
}
