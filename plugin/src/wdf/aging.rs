use super::*;


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
    pub(in crate::wdf) res_dev: [f64; MAX_RES],
    pub(in crate::wdf) cap_dev: [f64; MAX_CAPS],
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
    pub(in crate::wdf) mu0: f64,    // vacuum permeability
    pub(in crate::wdf) k_phi: f64,  // volts→flux lump: phi = k_phi * B  (= N_p * A_e)
    pub(in crate::wdf) k_hi: f64,   // current→field lump: H = k_hi * i_m (= N_p / l_e)
    pub(in crate::wdf) lp_eff: f64, // small-signal magnetizing inductance implied by the constants (for the deviation)
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
    pub(in crate::wdf) fn dmdb(&self, b: f64, m: f64, delta: f64) -> f64 {
        let h = b / self.mu0 - m;
        let s = self.slope(h, m, delta);
        s / (self.mu0 * (1.0 + s))
    }
}

