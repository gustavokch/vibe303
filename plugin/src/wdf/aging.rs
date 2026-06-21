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

/// Standard-normal sample (one of the Box-Muller pair). Deterministic, alloc-free; block-rate only.
#[inline]
fn next_gaussian(state: &mut u64) -> f64 {
    let u1 = next_unit(state).max(1.0e-12); // guard ln(0)
    let u2 = next_unit(state);
    // Box-Muller yields a pair (cos + sin); we keep only the cos member and discard the sin. Caching
    // the spare would alter the draw sequence (hence the voicing) for no real gain — this is a
    // block-rate, build-time-only generator, so the extra `next_unit` is free.
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// Truncated-normal signed deviation in `[-band, band]`. σ = band/2 so the band is ≈2σ (~95th
/// pctile) — most virtual units cluster near nominal, few reach the edge (vs the old flat uniform).
/// Resampled to the bound, clamped on fallback. `band <= 0` => exactly 0 (Ideal grade / no drift).
#[inline]
fn trunc_normal(state: &mut u64, band: f64) -> f64 {
    if band <= 0.0 {
        return 0.0;
    }
    let sigma = band * 0.5;
    let mut x = next_gaussian(state) * sigma;
    let mut tries = 0;
    while x.abs() > band && tries < 6 {
        x = next_gaussian(state) * sigma;
        tries += 1;
    }
    x.clamp(-band, band)
}

/// Derive a fresh aging seed from the current one (one re-roll step). Deterministic.
pub fn reroll_seed(seed: u64) -> u64 {
    let mut s = seed;
    splitmix64(&mut s)
}

/// Per-"unit" component drift deviations, derived once from a seed and independent of the `age`
/// knob (which only scales them at apply time). All are signed fractional deviations; unused array
/// slots stay 0.0. `ind_dev` ages the transformer coupled inductor (Lp, Ls); `core_dev` ages the
/// nonlinear JA core (m_s, k, c).
#[derive(Clone, Copy, Debug)]
pub struct AgePattern {
    pub(in crate::wdf) res_dev: [f64; MAX_RES],
    pub(in crate::wdf) cap_dev: [f64; MAX_CAPS],
    pub(in crate::wdf) ind_dev: [f64; 2],
    pub(in crate::wdf) core_dev: [f64; 3],
}

impl AgePattern {
    /// Identity pattern: zero drift everywhere (age has no effect).
    pub fn identity() -> Self {
        AgePattern {
            res_dev: [0.0; MAX_RES],
            cap_dev: [0.0; MAX_CAPS],
            ind_dev: [0.0; 2],
            core_dev: [0.0; 3],
        }
    }

    /// Generate per-unit deviations from `seed`, truncated-Gaussian, with each band set by the
    /// component's material (`res_mat`/`cap_mat`) and scaled by the build `grade`. Electrolytics get
    /// a directional (dry-out) draw; everything else is symmetric. `Ideal` grade => identity.
    pub(in crate::wdf) fn from_seed(
        seed: u64,
        n_res: usize,
        n_caps: usize,
        res_mat: &[ResMat; MAX_RES],
        cap_mat: &[CapMat; MAX_CAPS],
        grade: TolGrade,
    ) -> Self {
        let mut s = seed ^ 0xA283_A283_A283_A283; // domain-separate
        let mut p = AgePattern::identity();
        let g = grade.mult();
        for i in 0..n_res {
            p.res_dev[i] = trunc_normal(&mut s, res_mat[i].band() * g);
        }
        for i in 0..n_caps {
            p.cap_dev[i] = match cap_mat[i] {
                CapMat::Electrolytic => {
                    // Dry-out: directional and downward only — electrolytics lose capacitance as the
                    // gel dries, they don't gain it. Half-normal magnitude toward the drop side
                    // (σ so 2σ = dmax), bounded to the drop band: result in [-dmax, 0].
                    let dmax = C_ELEC_DROP_MAX * g;
                    if dmax <= 0.0 {
                        0.0
                    } else {
                        let mag = (next_gaussian(&mut s).abs() * (dmax * 0.5)).min(dmax);
                        -mag
                    }
                }
                other => trunc_normal(&mut s, other.band() * g),
            };
        }
        // Transformer coupled inductor (Lp, Ls).
        p.ind_dev[0] = trunc_normal(&mut s, L_DRIFT_MAX * g);
        p.ind_dev[1] = trunc_normal(&mut s, L_DRIFT_MAX * g);
        // Nonlinear JA core (m_s knee, k pinning, c reversibility).
        p.core_dev[0] = trunc_normal(&mut s, JA_MS_DRIFT * g);
        p.core_dev[1] = trunc_normal(&mut s, JA_K_DRIFT * g);
        p.core_dev[2] = trunc_normal(&mut s, JA_C_DRIFT * g);
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

/// Map a 0..1 dial with a fixed anchor at 0.5 onto a value: `lo` at 0, `mid` at 0.5, `hi` at 1.
/// Piecewise-linear so the default (0.5) lands EXACTLY on the voiced `mid` (no drift from today's
/// sound), with independent slopes either side so the clean and hot ends voice separately.
#[cfg(feature = "trafo-core")]
#[inline]
fn dial(d: f64, lo: f64, mid: f64, hi: f64) -> f64 {
    let d = d.clamp(0.0, 1.0);
    if d <= 0.5 {
        lo + (mid - lo) * (d / 0.5)
    } else {
        mid + (hi - mid) * ((d - 0.5) / 0.5)
    }
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
    /// continuity is unaffected (the ON/OFF switch sets `ja_active = false`, forcing `i_dev = 0`).
    /// To make the deviation truly vanish at low drive, pin `k_hi` to `s0` instead of `chi0` (uses `1.0 + s0`
    /// below) and re-run the `eval/trafo_core` B-H + THD harness — that change weakens `i_dev` ~40%
    /// at all levels, so it is a deliberate re-voice, left for a harness pass. No datasheet exists.
    pub fn lo1166() -> Self {
        let mu0 = std::f64::consts::PI * 4.0e-7;
        let alpha = 1.0e-5; // inter-domain coupling (small: chi0 ≈ chi_raw)
        let n_p = 100.0;
        let a_e = 5.0e-4;
        let k_phi = n_p * a_e; // volts→flux lump (= N_p·A_e)
        // Pin lp_eff to the linear magnetizing L (companion: Lp = 1.9 H) and back-solve k_hi from the
        // static initial susceptibility so the deviation is small at low drive. (Approximate — the
        // dynamic RK4 slope includes the reversibility factor `c`; see the doc-comment above.)
        let lp_eff = 1.9;
        // `set_voicing` holds `a = m_s/60`, so the anhysteretic susceptibility `chi_raw = m_s/(3a) =
        // 20` is constant across BOTH dials — and `k`/`c` don't enter `k_hi`. So the back-solved
        // current→field lump is constant: compute it once here from that fixed susceptibility.
        let chi_raw = JA_CHI_RAW;
        let chi0 = chi_raw / (1.0 - alpha * chi_raw);
        let k_hi = lp_eff / (k_phi * mu0 * (1.0 + chi0)); // current→field lump (= N_p/l_e)
        let mut core = JaCore { m_s: 0.0, a: 0.0, alpha, k: 0.0, c: 0.0, mu0, k_phi, k_hi, lp_eff };
        core.set_voicing(0.5, 0.5); // anchor at the voiced LO1166 default (b_sat=0.3T, k=300, c=0.7)
        core
    }

    /// Re-voice the core from the two front-panel dials (each 0..1, default 0.5 = the voiced LO1166).
    /// SATURATION moves the knee — `b_sat` → `m_s`, with `a = m_s/60` so `chi_raw` stays 20 and the
    /// back-solved `k_hi`/`lp_eff` lumps are unchanged. HYSTERESIS moves the loop width — `k` (pinning)
    /// and `c` (reversibility). At 0 each effect is removed (knee far above signal / near-anhysteretic
    /// no-loop), at 0.5 it is today's voicing, at 1 it exceeds it. Cheap: only `m_s, a, k, c` change —
    /// no lump rebuild, no Newton/partition contact (the J-A is a post-solve filter).
    pub fn set_voicing(&mut self, hyst: f64, sat: f64) {
        // SATURATION → knee flux density (T): clean (far above swing) .. today .. hard/early knee.
        let b_sat = dial(sat, 0.9, 0.3, 0.18);
        self.m_s = b_sat / self.mu0; // saturation magnetization (A/m); today ≈ 7.96e5
        self.a = self.m_s / 60.0; // hold chi_raw = m_s/(3a) = 20 (keeps k_hi/lp_eff valid)
        // HYSTERESIS → pinning + reversibility: near-anhysteretic (no loop) .. today .. wide loop.
        self.k = dial(hyst, 5.0, 300.0, 700.0); // A/m
        self.c = dial(hyst, 0.98, 0.7, 0.45); // reversibility 0..1
    }

    /// Retune the magnetizing-inductance lump after the primary `Lp` ages: re-pin `lp_eff` and
    /// re-back-solve `k_hi` from the fixed static susceptibility (chi_raw = 20, see `lo1166()`) so
    /// the low-drive deviation `(i_JA − phi/lp_eff)` keeps ≈cancelling. `lp = lp_base` => no change.
    pub fn set_lp_eff(&mut self, lp_eff: f64) {
        self.lp_eff = lp_eff;
        let chi_raw = JA_CHI_RAW;
        let chi0 = chi_raw / (1.0 - self.alpha * chi_raw);
        self.k_hi = lp_eff / (self.k_phi * self.mu0 * (1.0 + chi0));
    }

    /// Multiply the (already voiced) constitutive constants by per-unit aging factors. `f_ms` shifts
    /// the knee (`m_s`, holding `a = m_s/60` so `chi_raw = 20` and the lump stay valid), `f_k` the
    /// pinning, `f_c` the reversibility (clamped to [0,1]). All 1.0 => no change.
    pub fn apply_core_aging(&mut self, f_ms: f64, f_k: f64, f_c: f64) {
        self.m_s *= f_ms;
        self.a = self.m_s / 60.0;
        self.k *= f_k;
        self.c = (self.c * f_c).clamp(0.0, 1.0);
    }

    /// Test-only view of the aged constitutive constants `(m_s, k, c)`.
    #[cfg(test)]
    pub(in crate::wdf) fn core_msc(&self) -> (f64, f64, f64) {
        (self.m_s, self.k, self.c)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic material tables: res index 0 = MetalFilm, 1 = CarbonComp, rest CarbonComp;
    /// cap index 0 = Electrolytic, 1 = Styrene, rest Film.
    fn mats() -> ([ResMat; MAX_RES], [CapMat; MAX_CAPS]) {
        let mut r = [ResMat::CarbonComp; MAX_RES];
        r[0] = ResMat::MetalFilm;
        let mut c = [CapMat::Film; MAX_CAPS];
        c[0] = CapMat::Electrolytic;
        c[1] = CapMat::Styrene;
        (r, c)
    }

    #[test]
    fn ideal_grade_is_identity() {
        let (r, c) = mats();
        let p = AgePattern::from_seed(0x1234, MAX_RES, MAX_CAPS, &r, &c, TolGrade::Ideal);
        assert!(p.res_dev.iter().all(|&d| d == 0.0));
        assert!(p.cap_dev.iter().all(|&d| d == 0.0));
        assert!(p.ind_dev.iter().all(|&d| d == 0.0));
        assert!(p.core_dev.iter().all(|&d| d == 0.0));
    }

    #[test]
    fn bands_respected_standard() {
        let (r, c) = mats();
        for seed in 0..256u64 {
            let p = AgePattern::from_seed(seed, MAX_RES, MAX_CAPS, &r, &c, TolGrade::Standard);
            for i in 0..MAX_RES {
                assert!(p.res_dev[i].abs() <= r[i].band() + 1e-12, "res {i} seed {seed}");
            }
            // Electrolytic: directional, downward only, bounded to [-DROP_MAX, 0].
            assert!(p.cap_dev[0] <= 0.0);
            assert!(p.cap_dev[0] >= -C_ELEC_DROP_MAX - 1e-12);
            assert!(p.cap_dev[1].abs() <= CapMat::Styrene.band() + 1e-12);
            for i in 0..2 {
                assert!(p.ind_dev[i].abs() <= L_DRIFT_MAX + 1e-12);
            }
            assert!(p.core_dev[0].abs() <= JA_MS_DRIFT + 1e-12);
            assert!(p.core_dev[1].abs() <= JA_K_DRIFT + 1e-12);
            assert!(p.core_dev[2].abs() <= JA_C_DRIFT + 1e-12);
        }
    }

    #[test]
    fn carboncomp_drifts_more_than_metalfilm() {
        let (r, c) = mats();
        let (mut max_mf, mut max_cc) = (0.0f64, 0.0f64);
        for seed in 0..512u64 {
            let p = AgePattern::from_seed(seed, MAX_RES, MAX_CAPS, &r, &c, TolGrade::Standard);
            max_mf = max_mf.max(p.res_dev[0].abs()); // MetalFilm (±1%)
            max_cc = max_cc.max(p.res_dev[1].abs()); // CarbonComp (±10%)
        }
        assert!(max_cc > max_mf * 3.0, "carbon-comp {max_cc} not >> metal-film {max_mf}");
    }

    #[test]
    fn loose_wider_than_standard() {
        let (r, c) = mats();
        let (mut max_std, mut max_loose) = (0.0f64, 0.0f64);
        for seed in 0..512u64 {
            let ps = AgePattern::from_seed(seed, MAX_RES, MAX_CAPS, &r, &c, TolGrade::Standard);
            let pl = AgePattern::from_seed(seed, MAX_RES, MAX_CAPS, &r, &c, TolGrade::Loose);
            max_std = max_std.max(ps.res_dev[1].abs());
            max_loose = max_loose.max(pl.res_dev[1].abs());
        }
        assert!(max_loose > max_std * 1.5, "loose {max_loose} not wider than standard {max_std}");
    }

    #[test]
    fn gaussian_clusters_near_nominal() {
        // Mean |dev| of a truncated normal (σ = band/2) is ~0.40·band — well under a uniform's
        // 0.50·band, i.e. most draws sit near nominal.
        let band = 0.10;
        let mut s = 99u64;
        let mut sum = 0.0;
        let n = 20_000;
        for _ in 0..n {
            sum += trunc_normal(&mut s, band).abs();
        }
        let mean = sum / n as f64;
        assert!(mean < 0.5 * band * 0.9, "mean |dev| {mean} not clustered near nominal");
    }

    #[test]
    fn deterministic() {
        let (r, c) = mats();
        let a = AgePattern::from_seed(7, MAX_RES, MAX_CAPS, &r, &c, TolGrade::Standard);
        let b = AgePattern::from_seed(7, MAX_RES, MAX_CAPS, &r, &c, TolGrade::Standard);
        assert_eq!(a.res_dev, b.res_dev);
        assert_eq!(a.cap_dev, b.cap_dev);
        assert_eq!(a.ind_dev, b.ind_dev);
        assert_eq!(a.core_dev, b.core_dev);
    }
}
